// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!
* Bindings for the C standard library and other platform libraries
*
* This module contains bindings to the C standard library,
* organized into modules by their defining standard.
* Additionally, it contains some assorted platform-specific definitions.
* For convenience, most functions and types are reexported from `std::libc`,
* so `pub use std::libc::*` will import the available
* C bindings as appropriate for the target platform. The exact
* set of functions available are platform specific.
*
* *Note* Because these definitions are platform-specific, some may not appear in
* the generated documentation.
*
* We consider the following specs reasonably normative with respect
* to interoperating with the C standard library (libc/msvcrt):
*
* * ISO 9899:1990 ('C95', 'ANSI C', 'Standard C'), NA1, 1995.
* * ISO 9899:1999 ('C99' or 'C9x').
* * ISO 9945:1988 / IEEE 1003.1-1988 ('POSIX.1').
* * ISO 9945:2001 / IEEE 1003.1-2001 ('POSIX:2001', 'SUSv3').
* * ISO 9945:2008 / IEEE 1003.1-2008 ('POSIX:2008', 'SUSv4').
*
* Note that any reference to the 1996 revision of POSIX, or any revs
* between 1990 (when '88 was approved at ISO) and 2001 (when the next
* actual revision-revision happened), are merely additions of other
* chapters (1b and 1c) outside the core interfaces.
*
* Despite having several names each, these are *reasonably* coherent
* point-in-time, list-of-definition sorts of specs. You can get each under a
* variety of names but will wind up with the same definition in each case.
*
* See standards(7) in linux-manpages for more details.
*
* Our interface to these libraries is complicated by the non-universality of
* conformance to any of them. About the only thing universally supported is
* the first (C95), beyond that definitions quickly become absent on various
* platforms.
*
* We therefore wind up dividing our module-space up (mostly for the sake of
* sanity while editing, filling-in-details and eliminating duplication) into
* definitions common-to-all (held in modules named c95, c99, posix88, posix01
* and posix08) and definitions that appear only on *some* platforms (named
* 'extra'). This would be things like significant OSX foundation kit, or
* win32 library kernel32.dll, or various fancy glibc, linux or BSD
* extensions.
*
* In addition to the per-platform 'extra' modules, we define a module of
* 'common BSD' libc routines that never quite made it into POSIX but show up
* in multiple derived systems. This is the 4.4BSD r2 / 1995 release, the
* final one from Berkeley after the lawsuits died down and the CSRG
* dissolved.
*/

#[allow(non_camel_case_types)];
#[allow(non_uppercase_statics)];
#[allow(missing_doc)];

// Initial glob-exports mean that all the contents of all the modules
// wind up exported, if you're interested in writing platform-specific code.

pub use libc::types::common::c95::*;
pub use libc::types::common::c99::*;
pub use libc::types::common::posix88::*;
pub use libc::types::common::posix01::*;
pub use libc::types::common::posix08::*;
pub use libc::types::common::bsd44::*;
pub use libc::types::os::common::posix01::*;
pub use libc::types::os::arch::c95::*;
pub use libc::types::os::arch::c99::*;
pub use libc::types::os::arch::posix88::*;
pub use libc::types::os::arch::posix01::*;
pub use libc::types::os::arch::posix08::*;
pub use libc::types::os::arch::bsd44::*;
pub use libc::types::os::arch::extra::*;

pub use libc::consts::os::c95::*;
pub use libc::consts::os::c99::*;
pub use libc::consts::os::posix88::*;
pub use libc::consts::os::posix01::*;
pub use libc::consts::os::posix08::*;
pub use libc::consts::os::bsd44::*;
pub use libc::consts::os::extra::*;
pub use libc::consts::os::sysconf::*;

pub use libc::funcs::c95::ctype::*;
pub use libc::funcs::c95::stdio::*;
pub use libc::funcs::c95::stdlib::*;
pub use libc::funcs::c95::string::*;

pub use libc::funcs::posix88::stat_::*;
pub use libc::funcs::posix88::stdio::*;
pub use libc::funcs::posix88::fcntl::*;
pub use libc::funcs::posix88::dirent::*;
pub use libc::funcs::posix88::unistd::*;
pub use libc::funcs::posix88::mman::*;

pub use libc::funcs::posix01::stat_::*;
pub use libc::funcs::posix01::unistd::*;
pub use libc::funcs::posix01::glob::*;
pub use libc::funcs::posix01::mman::*;
pub use libc::funcs::posix08::unistd::*;

pub use libc::funcs::bsd44::*;
pub use libc::funcs::extra::*;

#[cfg(target_os = "win32")]
pub use libc::funcs::extra::kernel32::*;
#[cfg(target_os = "win32")]
pub use libc::funcs::extra::msvcrt::*;

// Explicit export lists for the intersection (provided here) mean that
// you can write more-platform-agnostic code if you stick to just these
// symbols.

pub use libc::types::common::c95::{FILE, c_void, fpos_t};
pub use libc::types::common::posix88::{DIR, dirent_t};
pub use libc::types::os::arch::c95::{c_char, c_double, c_float, c_int};
pub use libc::types::os::arch::c95::{c_long, c_short, c_uchar, c_ulong};
pub use libc::types::os::arch::c95::{c_ushort, clock_t, ptrdiff_t};
pub use libc::types::os::arch::c95::{size_t, time_t};
pub use libc::types::os::arch::c99::{c_longlong, c_ulonglong, intptr_t};
pub use libc::types::os::arch::c99::{uintptr_t};
pub use libc::types::os::arch::posix88::{dev_t, dirent_t, ino_t, mode_t};
pub use libc::types::os::arch::posix88::{off_t, pid_t, ssize_t};

pub use libc::consts::os::c95::{_IOFBF, _IOLBF, _IONBF, BUFSIZ, EOF};
pub use libc::consts::os::c95::{EXIT_FAILURE, EXIT_SUCCESS};
pub use libc::consts::os::c95::{FILENAME_MAX, FOPEN_MAX, L_tmpnam};
pub use libc::consts::os::c95::{RAND_MAX, SEEK_CUR, SEEK_END};
pub use libc::consts::os::c95::{SEEK_SET, TMP_MAX};
pub use libc::consts::os::posix88::{F_OK, O_APPEND, O_CREAT, O_EXCL};
pub use libc::consts::os::posix88::{O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY};
pub use libc::consts::os::posix88::{R_OK, S_IEXEC, S_IFBLK, S_IFCHR};
pub use libc::consts::os::posix88::{S_IFDIR, S_IFIFO, S_IFMT, S_IFREG};
pub use libc::consts::os::posix88::{S_IREAD, S_IRUSR, S_IRWXU, S_IWUSR};
pub use libc::consts::os::posix88::{STDERR_FILENO, STDIN_FILENO};
pub use libc::consts::os::posix88::{STDOUT_FILENO, W_OK, X_OK};

pub use libc::funcs::c95::ctype::{isalnum, isalpha, iscntrl, isdigit};
pub use libc::funcs::c95::ctype::{islower, isprint, ispunct, isspace};
pub use libc::funcs::c95::ctype::{isupper, isxdigit, tolower, toupper};

pub use libc::funcs::c95::stdio::{fclose, feof, ferror, fflush, fgetc};
pub use libc::funcs::c95::stdio::{fgetpos, fgets, fopen, fputc, fputs};
pub use libc::funcs::c95::stdio::{fread, freopen, fseek, fsetpos, ftell};
pub use libc::funcs::c95::stdio::{fwrite, perror, puts, remove, rewind};
pub use libc::funcs::c95::stdio::{setbuf, setvbuf, tmpfile, ungetc};

pub use libc::funcs::c95::stdlib::{abort, abs, atof, atoi, calloc, exit};
pub use libc::funcs::c95::stdlib::{free, getenv, labs, malloc, rand};
pub use libc::funcs::c95::stdlib::{realloc, srand, strtod, strtol};
pub use libc::funcs::c95::stdlib::{strtoul, system};

pub use libc::funcs::c95::string::{memchr, memcmp};
pub use libc::funcs::c95::string::{strcat, strchr, strcmp};
pub use libc::funcs::c95::string::{strcoll, strcpy, strcspn, strerror};
pub use libc::funcs::c95::string::{strlen, strncat, strncmp, strncpy};
pub use libc::funcs::c95::string::{strpbrk, strrchr, strspn, strstr};
pub use libc::funcs::c95::string::{strtok, strxfrm};

pub use libc::funcs::posix88::fcntl::{open, creat};
pub use libc::funcs::posix88::stat_::{chmod, fstat, mkdir, stat};
pub use libc::funcs::posix88::stdio::{fdopen, fileno, pclose, popen};
pub use libc::funcs::posix88::unistd::{access, chdir, close, dup, dup2};
pub use libc::funcs::posix88::unistd::{execv, execve, execvp, getcwd};
pub use libc::funcs::posix88::unistd::{getpid, isatty, lseek, pipe, read};
pub use libc::funcs::posix88::unistd::{rmdir, unlink, write};


pub mod types {

    // Types tend to vary *per architecture* so we pull their definitions out
    // into this module.

    // Standard types that are opaque or common, so are not per-target.
    pub mod common {
        pub mod c95 {
            /**
            Type used to construct void pointers for use with C.

            This type is only useful as a pointer target. Do not use it as a
            return type for FFI functions which have the `void` return type in
            C. Use the unit type `()` or omit the return type instead.
            */
            pub enum c_void {}
            pub enum FILE {}
            pub enum fpos_t {}
        }
        pub mod c99 {
            pub type int8_t = i8;
            pub type int16_t = i16;
            pub type int32_t = i32;
            pub type int64_t = i64;
            pub type uint8_t = u8;
            pub type uint16_t = u16;
            pub type uint32_t = u32;
            pub type uint64_t = u64;
        }
        pub mod posix88 {
            pub enum DIR {}
            pub enum dirent_t {}
        }
        pub mod posix01 {}
        pub mod posix08 {}
        pub mod bsd44 {}
    }

    // Standard types that are scalar but vary by OS and arch.

    #[cfg(target_os = "linux")]
    #[cfg(target_os = "android")]
    pub mod os {
        pub mod common {
            pub mod posix01 {
                use libc::types::common::c95::{c_void};
                use libc::types::os::arch::c95::{c_char, size_t};
                pub struct glob_t {
                    gl_pathc: size_t,
                    gl_pathv: **c_char,
                    gl_offs:  size_t,

                    __unused1: *c_void,
                    __unused2: *c_void,
                    __unused3: *c_void,
                    __unused4: *c_void,
                    __unused5: *c_void,
                }
            }
        }

        #[cfg(target_arch = "x86")]
        #[cfg(target_arch = "arm")]
        #[cfg(target_arch = "mips")]
        pub mod arch {
            pub mod c95 {
                pub type c_char = i8;
                pub type c_schar = i8;
                pub type c_uchar = u8;
                pub type c_short = i16;
                pub type c_ushort = u16;
                pub type c_int = i32;
                pub type c_uint = u32;
                pub type c_long = i32;
                pub type c_ulong = u32;
                pub type c_float = f32;
                pub type c_double = f64;
                pub type size_t = u32;
                pub type ptrdiff_t = i32;
                pub type clock_t = i32;
                pub type time_t = i32;
                pub type wchar_t = i32;
            }
            pub mod c99 {
                pub type c_longlong = i64;
                pub type c_ulonglong = u64;
                pub type intptr_t = int;
                pub type uintptr_t = uint;
            }
            #[cfg(target_arch = "x86")]
            #[cfg(target_arch = "mips")]
            pub mod posix88 {
                pub type off_t = i32;
                pub type dev_t = u64;
                pub type ino_t = u32;
                pub type pid_t = i32;
                pub type uid_t = u32;
                pub type gid_t = u32;
                pub type useconds_t = u32;
                pub type mode_t = u32;
                pub type ssize_t = i32;
            }
            #[cfg(target_arch = "arm")]
            pub mod posix88 {
                pub type off_t = i32;
                pub type dev_t = u32;
                pub type ino_t = u32;
                pub type pid_t = i32;
                pub type uid_t = u32;
                pub type gid_t = u32;
                pub type useconds_t = u32;
                pub type mode_t = u16;
                pub type ssize_t = i32;
            }
            #[cfg(target_arch = "x86")]
            pub mod posix01 {
                use libc::types::os::arch::c95::{c_short, c_long, time_t};
                use libc::types::os::arch::posix88::{dev_t, gid_t, ino_t};
                use libc::types::os::arch::posix88::{mode_t, off_t};
                use libc::types::os::arch::posix88::{uid_t};

                pub type nlink_t = u32;
                pub type blksize_t = i32;
                pub type blkcnt_t = i32;

                pub struct stat {
                    st_dev: dev_t,
                    __pad1: c_short,
                    st_ino: ino_t,
                    st_mode: mode_t,
                    st_nlink: nlink_t,
                    st_uid: uid_t,
                    st_gid: gid_t,
                    st_rdev: dev_t,
                    __pad2: c_short,
                    st_size: off_t,
                    st_blksize: blksize_t,
                    st_blocks: blkcnt_t,
                    st_atime: time_t,
                    st_atime_nsec: c_long,
                    st_mtime: time_t,
                    st_mtime_nsec: c_long,
                    st_ctime: time_t,
                    st_ctime_nsec: c_long,
                    __unused4: c_long,
                    __unused5: c_long,
                }
            }
            #[cfg(target_arch = "arm")]
            pub mod posix01 {
                use libc::types::os::arch::c95::{c_uchar, c_uint, c_ulong, time_t};
                use libc::types::os::arch::c99::{c_longlong, c_ulonglong};
                use libc::types::os::arch::posix88::{uid_t, gid_t, ino_t};

                pub type nlink_t = u16;
                pub type blksize_t = u32;
                pub type blkcnt_t = u32;

                pub struct stat {
                    st_dev: c_ulonglong,
                    __pad0: [c_uchar, ..4],
                    __st_ino: ino_t,
                    st_mode: c_uint,
                    st_nlink: c_uint,
                    st_uid: uid_t,
                    st_gid: gid_t,
                    st_rdev: c_ulonglong,
                    __pad3: [c_uchar, ..4],
                    st_size: c_longlong,
                    st_blksize: blksize_t,
                    st_blocks: c_ulonglong,
                    st_atime: time_t,
                    st_atime_nsec: c_ulong,
                    st_mtime: time_t,
                    st_mtime_nsec: c_ulong,
                    st_ctime: time_t,
                    st_ctime_nsec: c_ulong,
                    st_ino: c_ulonglong
                }
            }
            #[cfg(target_arch = "mips")]
            pub mod posix01 {
                use libc::types::os::arch::c95::{c_long, c_ulong, time_t};
                use libc::types::os::arch::posix88::{gid_t, ino_t};
                use libc::types::os::arch::posix88::{mode_t, off_t};
                use libc::types::os::arch::posix88::{uid_t};

                pub type nlink_t = u32;
                pub type blksize_t = i32;
                pub type blkcnt_t = i32;

                pub struct stat {
                    st_dev: c_ulong,
                    st_pad1: [c_long, ..3],
                    st_ino: ino_t,
                    st_mode: mode_t,
                    st_nlink: nlink_t,
                    st_uid: uid_t,
                    st_gid: gid_t,
                    st_rdev: c_ulong,
                    st_pad2: [c_long, ..2],
                    st_size: off_t,
                    st_pad3: c_long,
                    st_atime: time_t,
                    st_atime_nsec: c_long,
                    st_mtime: time_t,
                    st_mtime_nsec: c_long,
                    st_ctime: time_t,
                    st_ctime_nsec: c_long,
                    st_blksize: blksize_t,
                    st_blocks: blkcnt_t,
                    st_pad5: [c_long, ..14],
                }
            }
            pub mod posix08 {}
            pub mod bsd44 {}
            pub mod extra {}
        }

        #[cfg(target_arch = "x86_64")]
        pub mod arch {
            pub mod c95 {
                pub type c_char = i8;
                pub type c_schar = i8;
                pub type c_uchar = u8;
                pub type c_short = i16;
                pub type c_ushort = u16;
                pub type c_int = i32;
                pub type c_uint = u32;
                pub type c_long = i64;
                pub type c_ulong = u64;
                pub type c_float = f32;
                pub type c_double = f64;
                pub type size_t = u64;
                pub type ptrdiff_t = i64;
                pub type clock_t = i64;
                pub type time_t = i64;
                pub type wchar_t = i32;
            }
            pub mod c99 {
                pub type c_longlong = i64;
                pub type c_ulonglong = u64;
                pub type intptr_t = int;
                pub type uintptr_t = uint;
            }
            pub mod posix88 {
                pub type off_t = i64;
                pub type dev_t = u64;
                pub type ino_t = u64;
                pub type pid_t = i32;
                pub type uid_t = u32;
                pub type gid_t = u32;
                pub type useconds_t = u32;
                pub type mode_t = u32;
                pub type ssize_t = i64;
            }
            pub mod posix01 {
                use libc::types::os::arch::c95::{c_int, c_long, time_t};
                use libc::types::os::arch::posix88::{dev_t, gid_t, ino_t};
                use libc::types::os::arch::posix88::{mode_t, off_t};
                use libc::types::os::arch::posix88::{uid_t};

                pub type nlink_t = u64;
                pub type blksize_t = i64;
                pub type blkcnt_t = i64;
                pub struct stat {
                    st_dev: dev_t,
                    st_ino: ino_t,
                    st_nlink: nlink_t,
                    st_mode: mode_t,
                    st_uid: uid_t,
                    st_gid: gid_t,
                    __pad0: c_int,
                    st_rdev: dev_t,
                    st_size: off_t,
                    st_blksize: blksize_t,
                    st_blocks: blkcnt_t,
                    st_atime: time_t,
                    st_atime_nsec: c_long,
                    st_mtime: time_t,
                    st_mtime_nsec: c_long,
                    st_ctime: time_t,
                    st_ctime_nsec: c_long,
                    __unused: [c_long, ..3],
                }
            }
            pub mod posix08 {
            }
            pub mod bsd44 {
            }
            pub mod extra {
            }
        }
    }

    #[cfg(target_os = "freebsd")]
    pub mod os {
        pub mod common {
            pub mod posix01 {
                use libc::types::common::c95::{c_void};
                use libc::types::os::arch::c95::{c_char, c_int, size_t};
                pub struct glob_t {
                    gl_pathc:  size_t,
                    __unused1: size_t,
                    gl_offs:   size_t,
                    __unused2: c_int,
                    gl_pathv:  **c_char,

                    __unused3: *c_void,

                    __unused4: *c_void,
                    __unused5: *c_void,
                    __unused6: *c_void,
                    __unused7: *c_void,
                    __unused8: *c_void,
                }
            }
        }

        #[cfg(target_arch = "x86_64")]
        pub mod arch {
            pub mod c95 {
                pub type c_char = i8;
                pub type c_schar = i8;
                pub type c_uchar = u8;
                pub type c_short = i16;
                pub type c_ushort = u16;
                pub type c_int = i32;
                pub type c_uint = u32;
                pub type c_long = i64;
                pub type c_ulong = u64;
                pub type c_float = f32;
                pub type c_double = f64;
                pub type size_t = u64;
                pub type ptrdiff_t = i64;
                pub type clock_t = i32;
                pub type time_t = i64;
                pub type wchar_t = i32;
            }
            pub mod c99 {
                pub type c_longlong = i64;
                pub type c_ulonglong = u64;
                pub type intptr_t = int;
                pub type uintptr_t = uint;
            }
            pub mod posix88 {
                pub type off_t = i64;
                pub type dev_t = u32;
                pub type ino_t = u32;
                pub type pid_t = i32;
                pub type uid_t = u32;
                pub type gid_t = u32;
                pub type useconds_t = u32;
                pub type mode_t = u16;
                pub type ssize_t = i64;
            }
            pub mod posix01 {
                use libc::types::common::c99::{uint8_t, uint32_t, int32_t};
                use libc::types::os::arch::c95::{c_long, time_t};
                use libc::types::os::arch::posix88::{dev_t, gid_t, ino_t};
                use libc::types::os::arch::posix88::{mode_t, off_t};
                use libc::types::os::arch::posix88::{uid_t};

                pub type nlink_t = u16;
                pub type blksize_t = i64;
                pub type blkcnt_t = i64;
                pub type fflags_t = u32;
                pub struct stat {
                    st_dev: dev_t,
                    st_ino: ino_t,
                    st_mode: mode_t,
                    st_nlink: nlink_t,
                    st_uid: uid_t,
                    st_gid: gid_t,
                    st_rdev: dev_t,
                    st_atime: time_t,
                    st_atime_nsec: c_long,
                    st_mtime: time_t,
                    st_mtime_nsec: c_long,
                    st_ctime: time_t,
                    st_ctime_nsec: c_long,
                    st_size: off_t,
                    st_blocks: blkcnt_t,
                    st_blksize: blksize_t,
                    st_flags: fflags_t,
                    st_gen: uint32_t,
                    st_lspare: int32_t,
                    st_birthtime: time_t,
                    st_birthtime_nsec: c_long,
                    __unused: [uint8_t, ..2],
                }
            }
            pub mod posix08 {
            }
            pub mod bsd44 {
            }
            pub mod extra {
            }
        }
    }

    #[cfg(target_os = "win32")]
    pub mod os {
        pub mod common {
            pub mod posix01 {
                use libc::types::os::arch::c95::c_short;
                use libc::types::os::arch::extra::{int64, time64_t};
                use libc::types::os::arch::posix88::{dev_t, ino_t};
                use libc::types::os::arch::posix88::mode_t;

                // Note: this is the struct called stat64 in win32. Not stat,
                // nor stati64.
                pub struct stat {
                    st_dev: dev_t,
                    st_ino: ino_t,
                    st_mode: mode_t,
                    st_nlink: c_short,
                    st_uid: c_short,
                    st_gid: c_short,
                    st_rdev: dev_t,
                    st_size: int64,
                    st_atime: time64_t,
                    st_mtime: time64_t,
                    st_ctime: time64_t,
                }
            }
        }

        #[cfg(target_arch = "x86")]
        pub mod arch {
            pub mod c95 {
                pub type c_char = i8;
                pub type c_schar = i8;
                pub type c_uchar = u8;
                pub type c_short = i16;
                pub type c_ushort = u16;
                pub type c_int = i32;
                pub type c_uint = u32;
                pub type c_long = i32;
                pub type c_ulong = u32;
                pub type c_float = f32;
                pub type c_double = f64;
                pub type size_t = u32;
                pub type ptrdiff_t = i32;
                pub type clock_t = i32;
                pub type time_t = i32;
                pub type wchar_t = u16;
            }
            pub mod c99 {
                pub type c_longlong = i64;
                pub type c_ulonglong = u64;
                pub type intptr_t = int;
                pub type uintptr_t = uint;
            }
            pub mod posix88 {
                pub type off_t = i32;
                pub type dev_t = u32;
                pub type ino_t = i16;
                pub type pid_t = i32;
                pub type useconds_t = u32;
                pub type mode_t = u16;
                pub type ssize_t = i32;
            }
            pub mod posix01 {
            }
            pub mod posix08 {
            }
            pub mod bsd44 {
            }
            pub mod extra {
                use ptr;
                use libc::types::common::c95::c_void;
                use libc::types::os::arch::c95::{c_char, c_int, c_uint, size_t};
                use libc::types::os::arch::c95::{c_long, c_ulong};
                use libc::types::os::arch::c95::{wchar_t};
                use libc::types::os::arch::c99::{c_ulonglong};

                pub type BOOL = c_int;
                pub type BYTE = u8;
                pub type CCHAR = c_char;
                pub type CHAR = c_char;

                pub type DWORD = c_ulong;
                pub type DWORDLONG = c_ulonglong;

                pub type HANDLE = LPVOID;
                pub type HMODULE = c_uint;

                pub type LONG_PTR = c_long;

                pub type LPCWSTR = *WCHAR;
                pub type LPCSTR = *CHAR;
                pub type LPCTSTR = *CHAR;
                pub type LPTCH = *CHAR;

                pub type LPWSTR = *mut WCHAR;
                pub type LPSTR = *mut CHAR;
                pub type LPTSTR = *mut CHAR;

                // Not really, but opaque to us.
                pub type LPSECURITY_ATTRIBUTES = LPVOID;

                pub type LPVOID = *mut c_void;
                pub type LPCVOID = *c_void;
                pub type LPBYTE = *mut BYTE;
                pub type LPWORD = *mut WORD;
                pub type LPDWORD = *mut DWORD;
                pub type LPHANDLE = *mut HANDLE;

                pub type LRESULT = LONG_PTR;
                pub type PBOOL = *mut BOOL;
                pub type WCHAR = wchar_t;
                pub type WORD = u16;
                pub type SIZE_T = size_t;

                pub type time64_t = i64;
                pub type int64 = i64;

                pub struct STARTUPINFO {
                    cb: DWORD,
                    lpReserved: LPTSTR,
                    lpDesktop: LPTSTR,
                    lpTitle: LPTSTR,
                    dwX: DWORD,
                    dwY: DWORD,
                    dwXSize: DWORD,
                    dwYSize: DWORD,
                    dwXCountChars: DWORD,
                    dwYCountCharts: DWORD,
                    dwFillAttribute: DWORD,
                    dwFlags: DWORD,
                    wShowWindow: WORD,
                    cbReserved2: WORD,
                    lpReserved2: LPBYTE,
                    hStdInput: HANDLE,
                    hStdOutput: HANDLE,
                    hStdError: HANDLE
                }
                pub type LPSTARTUPINFO = *mut STARTUPINFO;

                pub struct PROCESS_INFORMATION {
                    hProcess: HANDLE,
                    hThread: HANDLE,
                    dwProcessId: DWORD,
                    dwThreadId: DWORD
                }
                pub type LPPROCESS_INFORMATION = *mut PROCESS_INFORMATION;

                pub struct SYSTEM_INFO {
                    wProcessorArchitecture: WORD,
                    wReserved: WORD,
                    dwPageSize: DWORD,
                    lpMinimumApplicationAddress: LPVOID,
                    lpMaximumApplicationAddress: LPVOID,
                    dwActiveProcessorMask: DWORD,
                    dwNumberOfProcessors: DWORD,
                    dwProcessorType: DWORD,
                    dwAllocationGranularity: DWORD,
                    wProcessorLevel: WORD,
                    wProcessorRevision: WORD
                }
                pub type LPSYSTEM_INFO = *mut SYSTEM_INFO;

                impl SYSTEM_INFO {
                    pub fn new() -> SYSTEM_INFO {
                        SYSTEM_INFO {
                            wProcessorArchitecture: 0,
                            wReserved: 0,
                            dwPageSize: 0,
                            lpMinimumApplicationAddress: ptr::mut_null(),
                            lpMaximumApplicationAddress: ptr::mut_null(),
                            dwActiveProcessorMask: 0,
                            dwNumberOfProcessors: 0,
                            dwProcessorType: 0,
                            dwAllocationGranularity: 0,
                            wProcessorLevel: 0,
                            wProcessorRevision: 0
                        }
                    }
                }

                pub struct MEMORY_BASIC_INFORMATION {
                    BaseAddress: LPVOID,
                    AllocationBase: LPVOID,
                    AllocationProtect: DWORD,
                    RegionSize: SIZE_T,
                    State: DWORD,
                    Protect: DWORD,
                    Type: DWORD
                }
                pub type LPMEMORY_BASIC_INFORMATION = *mut MEMORY_BASIC_INFORMATION;
            }
        }

        #[cfg(target_arch = "x86_64")]
        pub mod arch {
            pub mod c95 {
                pub type c_char = i8;
                pub type c_schar = i8;
                pub type c_uchar = u8;
                pub type c_short = i16;
                pub type c_ushort = u16;
                pub type c_int = i32;
                pub type c_uint = u32;
                pub type c_long = i32;
                pub type c_ulong = u32;
                pub type c_float = f32;
                pub type c_double = f64;
                pub type size_t = u64;
                pub type ptrdiff_t = i64;
                pub type clock_t = i32;
                pub type time_t = i64;
                pub type wchar_t = u16;
            }
            pub mod c99 {
                pub type c_longlong = i64;
                pub type c_ulonglong = u64;
                pub type intptr_t = int;
                pub type uintptr_t = uint;
            }
            pub mod posix88 {
                pub type off_t = i32; // XXX unless _FILE_OFFSET_BITS == 64
                pub type dev_t = u32;
                pub type ino_t = i16;
                pub type pid_t = i64;
                pub type useconds_t = u32;
                pub type mode_t = u16;
                pub type ssize_t = i64;
            }
            pub mod posix01 {
            }
            pub mod posix08 {
            }
            pub mod bsd44 {
            }
            pub mod extra {
                use ptr;
                use libc::types::common::c95::c_void;
                use libc::types::os::arch::c95::{c_char, c_int, c_uint, size_t};
                use libc::types::os::arch::c95::{c_ulong};
                use libc::types::os::arch::c95::{wchar_t};
                use libc::types::os::arch::c99::{c_ulonglong};

                pub type BOOL = c_int;
                pub type BYTE = u8;
                pub type CCHAR = c_char;
                pub type CHAR = c_char;

                pub type DWORD = c_ulong;
                pub type DWORDLONG = c_ulonglong;

                pub type HANDLE = LPVOID;
                pub type HMODULE = c_uint;

                pub type LONG_PTR = i64; // changed

                pub type LPCWSTR = *WCHAR;
                pub type LPCSTR = *CHAR;
                pub type LPCTSTR = *CHAR;
                pub type LPTCH = *CHAR;

                pub type LPWSTR = *mut WCHAR;
                pub type LPSTR = *mut CHAR;
                pub type LPTSTR = *mut CHAR;

                // Not really, but opaque to us.
                pub type LPSECURITY_ATTRIBUTES = LPVOID;

                pub type LPVOID = *mut c_void;
                pub type LPCVOID = *c_void;
                pub type LPBYTE = *mut BYTE;
                pub type LPWORD = *mut WORD;
                pub type LPDWORD = *mut DWORD;
                pub type LPHANDLE = *mut HANDLE;

                pub type LRESULT = LONG_PTR;
                pub type PBOOL = *mut BOOL;
                pub type WCHAR = wchar_t;
                pub type WORD = u16;
                pub type SIZE_T = size_t;

                pub type time64_t = i64;
                pub type int64 = i64;

                pub struct STARTUPINFO {
                    cb: DWORD,
                    lpReserved: LPTSTR,
                    lpDesktop: LPTSTR,
                    lpTitle: LPTSTR,
                    dwX: DWORD,
                    dwY: DWORD,
                    dwXSize: DWORD,
                    dwYSize: DWORD,
                    dwXCountChars: DWORD,
                    dwYCountCharts: DWORD,
                    dwFillAttribute: DWORD,
                    dwFlags: DWORD,
                    wShowWindow: WORD,
                    cbReserved2: WORD,
                    lpReserved2: LPBYTE,
                    hStdInput: HANDLE,
                    hStdOutput: HANDLE,
                    hStdError: HANDLE
                }
                pub type LPSTARTUPINFO = *mut STARTUPINFO;

                pub struct PROCESS_INFORMATION {
                    hProcess: HANDLE,
                    hThread: HANDLE,
                    dwProcessId: DWORD,
                    dwThreadId: DWORD
                }
                pub type LPPROCESS_INFORMATION = *mut PROCESS_INFORMATION;

                pub struct SYSTEM_INFO {
                    wProcessorArchitecture: WORD,
                    wReserved: WORD,
                    dwPageSize: DWORD,
                    lpMinimumApplicationAddress: LPVOID,
                    lpMaximumApplicationAddress: LPVOID,
                    dwActiveProcessorMask: DWORD,
                    dwNumberOfProcessors: DWORD,
                    dwProcessorType: DWORD,
                    dwAllocationGranularity: DWORD,
                    wProcessorLevel: WORD,
                    wProcessorRevision: WORD
                }
                pub type LPSYSTEM_INFO = *mut SYSTEM_INFO;

                impl SYSTEM_INFO {
                    pub fn new() -> SYSTEM_INFO {
                        SYSTEM_INFO {
                            wProcessorArchitecture: 0,
                            wReserved: 0,
                            dwPageSize: 0,
                            lpMinimumApplicationAddress: ptr::mut_null(),
                            lpMaximumApplicationAddress: ptr::mut_null(),
                            dwActiveProcessorMask: 0,
                            dwNumberOfProcessors: 0,
                            dwProcessorType: 0,
                            dwAllocationGranularity: 0,
                            wProcessorLevel: 0,
                            wProcessorRevision: 0
                        }
                    }
                }

                pub struct MEMORY_BASIC_INFORMATION {
                    BaseAddress: LPVOID,
                    AllocationBase: LPVOID,
                    AllocationProtect: DWORD,
                    RegionSize: SIZE_T,
                    State: DWORD,
                    Protect: DWORD,
                    Type: DWORD
                }
                pub type LPMEMORY_BASIC_INFORMATION = *mut MEMORY_BASIC_INFORMATION;
            }
        }
    }

    #[cfg(target_os = "macos")]
    pub mod os {
        pub mod common {
            pub mod posix01 {
                use libc::types::common::c95::{c_void};
                use libc::types::os::arch::c95::{c_char, c_int, size_t};
                pub struct glob_t {
                    gl_pathc:  size_t,
                    __unused1: c_int,
                    gl_offs:   size_t,
                    __unused2: c_int,
                    gl_pathv:  **c_char,

                    __unused3: *c_void,

                    __unused4: *c_void,
                    __unused5: *c_void,
                    __unused6: *c_void,
                    __unused7: *c_void,
                    __unused8: *c_void,
                }
            }
        }

        #[cfg(target_arch = "x86")]
        pub mod arch {
            pub mod c95 {
                pub type c_char = i8;
                pub type c_schar = i8;
                pub type c_uchar = u8;
                pub type c_short = i16;
                pub type c_ushort = u16;
                pub type c_int = i32;
                pub type c_uint = u32;
                pub type c_long = i32;
                pub type c_ulong = u32;
                pub type c_float = f32;
                pub type c_double = f64;
                pub type size_t = u32;
                pub type ptrdiff_t = i32;
                pub type clock_t = u32;
                pub type time_t = i32;
                pub type wchar_t = i32;
            }
            pub mod c99 {
                pub type c_longlong = i64;
                pub type c_ulonglong = u64;
                pub type intptr_t = int;
                pub type uintptr_t = uint;
            }
            pub mod posix88 {
                pub type off_t = i64;
                pub type dev_t = i32;
                pub type ino_t = u64;
                pub type pid_t = i32;
                pub type uid_t = u32;
                pub type gid_t = u32;
                pub type useconds_t = u32;
                pub type mode_t = u16;
                pub type ssize_t = i32;
            }
            pub mod posix01 {
                use libc::types::common::c99::{int32_t, int64_t, uint32_t};
                use libc::types::os::arch::c95::{c_long, time_t};
                use libc::types::os::arch::posix88::{dev_t, gid_t, ino_t,
                                                     mode_t, off_t, uid_t};

                pub type nlink_t = u16;
                pub type blksize_t = i64;
                pub type blkcnt_t = i32;

                pub struct stat {
                    st_dev: dev_t,
                    st_mode: mode_t,
                    st_nlink: nlink_t,
                    st_ino: ino_t,
                    st_uid: uid_t,
                    st_gid: gid_t,
                    st_rdev: dev_t,
                    st_atime: time_t,
                    st_atime_nsec: c_long,
                    st_mtime: time_t,
                    st_mtime_nsec: c_long,
                    st_ctime: time_t,
                    st_ctime_nsec: c_long,
                    st_birthtime: time_t,
                    st_birthtime_nsec: c_long,
                    st_size: off_t,
                    st_blocks: blkcnt_t,
                    st_blksize: blksize_t,
                    st_flags: uint32_t,
                    st_gen: uint32_t,
                    st_lspare: int32_t,
                    st_qspare: [int64_t, ..2],
                }
            }
            pub mod posix08 {
            }
            pub mod bsd44 {
            }
            pub mod extra {
            }
        }

        #[cfg(target_arch = "x86_64")]
        pub mod arch {
            pub mod c95 {
                pub type c_char = i8;
                pub type c_schar = i8;
                pub type c_uchar = u8;
                pub type c_short = i16;
                pub type c_ushort = u16;
                pub type c_int = i32;
                pub type c_uint = u32;
                pub type c_long = i64;
                pub type c_ulong = u64;
                pub type c_float = f32;
                pub type c_double = f64;
                pub type size_t = u64;
                pub type ptrdiff_t = i64;
                pub type clock_t = u64;
                pub type time_t = i64;
                pub type wchar_t = i32;
            }
            pub mod c99 {
                pub type c_longlong = i64;
                pub type c_ulonglong = u64;
                pub type intptr_t = int;
                pub type uintptr_t = uint;
            }
            pub mod posix88 {
                pub type off_t = i64;
                pub type dev_t = i32;
                pub type ino_t = u64;
                pub type pid_t = i32;
                pub type uid_t = u32;
                pub type gid_t = u32;
                pub type useconds_t = u32;
                pub type mode_t = u16;
                pub type ssize_t = i64;
            }
            pub mod posix01 {
                use libc::types::common::c99::{int32_t, int64_t};
                use libc::types::common::c99::{uint32_t};
                use libc::types::os::arch::c95::{c_long, time_t};
                use libc::types::os::arch::posix88::{dev_t, gid_t, ino_t};
                use libc::types::os::arch::posix88::{mode_t, off_t, uid_t};

                pub type nlink_t = u16;
                pub type blksize_t = i64;
                pub type blkcnt_t = i32;

                pub struct stat {
                    st_dev: dev_t,
                    st_mode: mode_t,
                    st_nlink: nlink_t,
                    st_ino: ino_t,
                    st_uid: uid_t,
                    st_gid: gid_t,
                    st_rdev: dev_t,
                    st_atime: time_t,
                    st_atime_nsec: c_long,
                    st_mtime: time_t,
                    st_mtime_nsec: c_long,
                    st_ctime: time_t,
                    st_ctime_nsec: c_long,
                    st_birthtime: time_t,
                    st_birthtime_nsec: c_long,
                    st_size: off_t,
                    st_blocks: blkcnt_t,
                    st_blksize: blksize_t,
                    st_flags: uint32_t,
                    st_gen: uint32_t,
                    st_lspare: int32_t,
                    st_qspare: [int64_t, ..2],
                }
            }
            pub mod posix08 {
            }
            pub mod bsd44 {
            }
            pub mod extra {
            }
        }
    }
}

pub mod consts {
    // Consts tend to vary per OS so we pull their definitions out
    // into this module.

    #[cfg(target_os = "win32")]
    pub mod os {
        pub mod c95 {
            use libc::types::os::arch::c95::{c_int, c_uint};

            pub static EXIT_FAILURE : c_int = 1;
            pub static EXIT_SUCCESS : c_int = 0;
            pub static RAND_MAX : c_int = 32767;
            pub static EOF : c_int = -1;
            pub static SEEK_SET : c_int = 0;
            pub static SEEK_CUR : c_int = 1;
            pub static SEEK_END : c_int = 2;
            pub static _IOFBF : c_int = 0;
            pub static _IONBF : c_int = 4;
            pub static _IOLBF : c_int = 64;
            pub static BUFSIZ : c_uint = 512_u32;
            pub static FOPEN_MAX : c_uint = 20_u32;
            pub static FILENAME_MAX : c_uint = 260_u32;
            pub static L_tmpnam : c_uint = 16_u32;
            pub static TMP_MAX : c_uint = 32767_u32;
        }
        pub mod c99 {
        }
        pub mod posix88 {
            use libc::types::os::arch::c95::c_int;

            pub static O_RDONLY : c_int = 0;
            pub static O_WRONLY : c_int = 1;
            pub static O_RDWR : c_int = 2;
            pub static O_APPEND : c_int = 8;
            pub static O_CREAT : c_int = 256;
            pub static O_EXCL : c_int = 1024;
            pub static O_TRUNC : c_int = 512;
            pub static S_IFIFO : c_int = 4096;
            pub static S_IFCHR : c_int = 8192;
            pub static S_IFBLK : c_int = 12288;
            pub static S_IFDIR : c_int = 16384;
            pub static S_IFREG : c_int = 32768;
            pub static S_IFMT : c_int = 61440;
            pub static S_IEXEC : c_int = 64;
            pub static S_IWRITE : c_int = 128;
            pub static S_IREAD : c_int = 256;
            pub static S_IRWXU : c_int = 448;
            pub static S_IXUSR : c_int = 64;
            pub static S_IWUSR : c_int = 128;
            pub static S_IRUSR : c_int = 256;
            pub static F_OK : c_int = 0;
            pub static R_OK : c_int = 4;
            pub static W_OK : c_int = 2;
            pub static X_OK : c_int = 1;
            pub static STDIN_FILENO : c_int = 0;
            pub static STDOUT_FILENO : c_int = 1;
            pub static STDERR_FILENO : c_int = 2;
        }
        pub mod posix01 {
        }
        pub mod posix08 {
        }
        pub mod bsd44 {
        }
        pub mod extra {
            use libc::types::os::arch::c95::c_int;
            use libc::types::os::arch::extra::{WORD, DWORD, BOOL};

            pub static TRUE : BOOL = 1;
            pub static FALSE : BOOL = 0;

            pub static O_TEXT : c_int = 16384;
            pub static O_BINARY : c_int = 32768;
            pub static O_NOINHERIT: c_int = 128;

            pub static ERROR_SUCCESS : c_int = 0;
            pub static ERROR_INVALID_HANDLE : c_int = 6;
            pub static ERROR_DISK_FULL : c_int = 112;
            pub static ERROR_INSUFFICIENT_BUFFER : c_int = 122;
            pub static ERROR_ALREADY_EXISTS : c_int = 183;
            pub static ERROR_INVALID_ADDRESS : c_int = 487;
            pub static ERROR_FILE_INVALID : c_int = 1006;
            pub static INVALID_HANDLE_VALUE : c_int = -1;

            pub static DELETE : DWORD = 0x00010000;
            pub static READ_CONTROL : DWORD = 0x00020000;
            pub static SYNCHRONIZE : DWORD = 0x00100000;
            pub static WRITE_DAC : DWORD = 0x00040000;
            pub static WRITE_OWNER : DWORD = 0x00080000;

            pub static PROCESS_CREATE_PROCESS : DWORD = 0x0080;
            pub static PROCESS_CREATE_THREAD : DWORD = 0x0002;
            pub static PROCESS_DUP_HANDLE : DWORD = 0x0040;
            pub static PROCESS_QUERY_INFORMATION : DWORD = 0x0400;
            pub static PROCESS_QUERY_LIMITED_INFORMATION : DWORD = 0x1000;
            pub static PROCESS_SET_INFORMATION : DWORD = 0x0200;
            pub static PROCESS_SET_QUOTA : DWORD = 0x0100;
            pub static PROCESS_SUSPEND_RESUME : DWORD = 0x0800;
            pub static PROCESS_TERMINATE : DWORD = 0x0001;
            pub static PROCESS_VM_OPERATION : DWORD = 0x0008;
            pub static PROCESS_VM_READ : DWORD = 0x0010;
            pub static PROCESS_VM_WRITE : DWORD = 0x0020;

            pub static STARTF_FORCEONFEEDBACK : DWORD = 0x00000040;
            pub static STARTF_FORCEOFFFEEDBACK : DWORD = 0x00000080;
            pub static STARTF_PREVENTPINNING : DWORD = 0x00002000;
            pub static STARTF_RUNFULLSCREEN : DWORD = 0x00000020;
            pub static STARTF_TITLEISAPPID : DWORD = 0x00001000;
            pub static STARTF_TITLEISLINKNAME : DWORD = 0x00000800;
            pub static STARTF_USECOUNTCHARS : DWORD = 0x00000008;
            pub static STARTF_USEFILLATTRIBUTE : DWORD = 0x00000010;
            pub static STARTF_USEHOTKEY : DWORD = 0x00000200;
            pub static STARTF_USEPOSITION : DWORD = 0x00000004;
            pub static STARTF_USESHOWWINDOW : DWORD = 0x00000001;
            pub static STARTF_USESIZE : DWORD = 0x00000002;
            pub static STARTF_USESTDHANDLES : DWORD = 0x00000100;

            pub static WAIT_ABANDONED : DWORD = 0x00000080;
            pub static WAIT_OBJECT_0 : DWORD = 0x00000000;
            pub static WAIT_TIMEOUT : DWORD = 0x00000102;
            pub static WAIT_FAILED : DWORD = -1;

            pub static DUPLICATE_CLOSE_SOURCE : DWORD = 0x00000001;
            pub static DUPLICATE_SAME_ACCESS : DWORD = 0x00000002;

            pub static INFINITE : DWORD = -1;
            pub static STILL_ACTIVE : DWORD = 259;

            pub static MEM_COMMIT : DWORD = 0x00001000;
            pub static MEM_RESERVE : DWORD = 0x00002000;
            pub static MEM_DECOMMIT : DWORD = 0x00004000;
            pub static MEM_RELEASE : DWORD = 0x00008000;
            pub static MEM_RESET : DWORD = 0x00080000;
            pub static MEM_RESET_UNDO : DWORD = 0x1000000;
            pub static MEM_LARGE_PAGES : DWORD = 0x20000000;
            pub static MEM_PHYSICAL : DWORD = 0x00400000;
            pub static MEM_TOP_DOWN : DWORD = 0x00100000;
            pub static MEM_WRITE_WATCH : DWORD = 0x00200000;

            pub static PAGE_EXECUTE : DWORD = 0x10;
            pub static PAGE_EXECUTE_READ : DWORD = 0x20;
            pub static PAGE_EXECUTE_READWRITE : DWORD = 0x40;
            pub static PAGE_EXECUTE_WRITECOPY : DWORD = 0x80;
            pub static PAGE_NOACCESS : DWORD = 0x01;
            pub static PAGE_READONLY : DWORD = 0x02;
            pub static PAGE_READWRITE : DWORD = 0x04;
            pub static PAGE_WRITECOPY : DWORD = 0x08;
            pub static PAGE_GUARD : DWORD = 0x100;
            pub static PAGE_NOCACHE : DWORD = 0x200;
            pub static PAGE_WRITECOMBINE : DWORD = 0x400;

            pub static SEC_COMMIT : DWORD = 0x8000000;
            pub static SEC_IMAGE : DWORD = 0x1000000;
            pub static SEC_IMAGE_NO_EXECUTE : DWORD = 0x11000000;
            pub static SEC_LARGE_PAGES : DWORD = 0x80000000;
            pub static SEC_NOCACHE : DWORD = 0x10000000;
            pub static SEC_RESERVE : DWORD = 0x4000000;
            pub static SEC_WRITECOMBINE : DWORD = 0x40000000;

            pub static FILE_MAP_ALL_ACCESS : DWORD = 0xf001f;
            pub static FILE_MAP_READ : DWORD = 0x4;
            pub static FILE_MAP_WRITE : DWORD = 0x2;
            pub static FILE_MAP_COPY : DWORD = 0x1;
            pub static FILE_MAP_EXECUTE : DWORD = 0x20;

            pub static PROCESSOR_ARCHITECTURE_INTEL : WORD = 0;
            pub static PROCESSOR_ARCHITECTURE_ARM : WORD = 5;
            pub static PROCESSOR_ARCHITECTURE_IA64 : WORD = 6;
            pub static PROCESSOR_ARCHITECTURE_AMD64 : WORD = 9;
            pub static PROCESSOR_ARCHITECTURE_UNKNOWN : WORD = 0xffff;
        }
        pub mod sysconf {
        }
    }


    #[cfg(target_os = "linux")]
    #[cfg(target_os = "android")]
    pub mod os {
        pub mod c95 {
            use libc::types::os::arch::c95::{c_int, c_uint};

            pub static EXIT_FAILURE : c_int = 1;
            pub static EXIT_SUCCESS : c_int = 0;
            pub static RAND_MAX : c_int = 2147483647;
            pub static EOF : c_int = -1;
            pub static SEEK_SET : c_int = 0;
            pub static SEEK_CUR : c_int = 1;
            pub static SEEK_END : c_int = 2;
            pub static _IOFBF : c_int = 0;
            pub static _IONBF : c_int = 2;
            pub static _IOLBF : c_int = 1;
            pub static BUFSIZ : c_uint = 8192_u32;
            pub static FOPEN_MAX : c_uint = 16_u32;
            pub static FILENAME_MAX : c_uint = 4096_u32;
            pub static L_tmpnam : c_uint = 20_u32;
            pub static TMP_MAX : c_uint = 238328_u32;
        }
        pub mod c99 {
        }
        #[cfg(target_arch = "x86")]
        #[cfg(target_arch = "x86_64")]
        #[cfg(target_arch = "arm")]
        pub mod posix88 {
            use libc::types::os::arch::c95::c_int;
            use libc::types::common::c95::c_void;

            pub static O_RDONLY : c_int = 0;
            pub static O_WRONLY : c_int = 1;
            pub static O_RDWR : c_int = 2;
            pub static O_APPEND : c_int = 1024;
            pub static O_CREAT : c_int = 64;
            pub static O_EXCL : c_int = 128;
            pub static O_TRUNC : c_int = 512;
            pub static S_IFIFO : c_int = 4096;
            pub static S_IFCHR : c_int = 8192;
            pub static S_IFBLK : c_int = 24576;
            pub static S_IFDIR : c_int = 16384;
            pub static S_IFREG : c_int = 32768;
            pub static S_IFMT : c_int = 61440;
            pub static S_IEXEC : c_int = 64;
            pub static S_IWRITE : c_int = 128;
            pub static S_IREAD : c_int = 256;
            pub static S_IRWXU : c_int = 448;
            pub static S_IXUSR : c_int = 64;
            pub static S_IWUSR : c_int = 128;
            pub static S_IRUSR : c_int = 256;
            pub static F_OK : c_int = 0;
            pub static R_OK : c_int = 4;
            pub static W_OK : c_int = 2;
            pub static X_OK : c_int = 1;
            pub static STDIN_FILENO : c_int = 0;
            pub static STDOUT_FILENO : c_int = 1;
            pub static STDERR_FILENO : c_int = 2;
            pub static F_LOCK : c_int = 1;
            pub static F_TEST : c_int = 3;
            pub static F_TLOCK : c_int = 2;
            pub static F_ULOCK : c_int = 0;
            pub static SIGHUP : c_int = 1;
            pub static SIGINT : c_int = 2;
            pub static SIGQUIT : c_int = 3;
            pub static SIGILL : c_int = 4;
            pub static SIGABRT : c_int = 6;
            pub static SIGFPE : c_int = 8;
            pub static SIGKILL : c_int = 9;
            pub static SIGSEGV : c_int = 11;
            pub static SIGPIPE : c_int = 13;
            pub static SIGALRM : c_int = 14;
            pub static SIGTERM : c_int = 15;

            pub static PROT_NONE : c_int = 0;
            pub static PROT_READ : c_int = 1;
            pub static PROT_WRITE : c_int = 2;
            pub static PROT_EXEC : c_int = 4;

            pub static MAP_FILE : c_int = 0x0000;
            pub static MAP_SHARED : c_int = 0x0001;
            pub static MAP_PRIVATE : c_int = 0x0002;
            pub static MAP_FIXED : c_int = 0x0010;
            pub static MAP_ANON : c_int = 0x0020;

            pub static MAP_FAILED : *c_void = -1 as *c_void;

            pub static MCL_CURRENT : c_int = 0x0001;
            pub static MCL_FUTURE : c_int = 0x0002;

            pub static MS_ASYNC : c_int = 0x0001;
            pub static MS_INVALIDATE : c_int = 0x0002;
            pub static MS_SYNC : c_int = 0x0004;

            pub static EPERM : c_int = 1;
            pub static ENOENT : c_int = 2;
            pub static ESRCH : c_int = 3;
            pub static EINTR : c_int = 4;
            pub static EIO : c_int = 5;
            pub static ENXIO : c_int = 6;
            pub static E2BIG : c_int = 7;
            pub static ENOEXEC : c_int = 8;
            pub static EBADF : c_int = 9;
            pub static ECHILD : c_int = 10;
            pub static EAGAIN : c_int = 11;
            pub static ENOMEM : c_int = 12;
            pub static EACCES : c_int = 13;
            pub static EFAULT : c_int = 14;
            pub static ENOTBLK : c_int = 15;
            pub static EBUSY : c_int = 16;
            pub static EEXIST : c_int = 17;
            pub static EXDEV : c_int = 18;
            pub static ENODEV : c_int = 19;
            pub static ENOTDIR : c_int = 20;
            pub static EISDIR : c_int = 21;
            pub static EINVAL : c_int = 22;
            pub static ENFILE : c_int = 23;
            pub static EMFILE : c_int = 24;
            pub static ENOTTY : c_int = 25;
            pub static ETXTBSY : c_int = 26;
            pub static EFBIG : c_int = 27;
            pub static ENOSPC : c_int = 28;
            pub static ESPIPE : c_int = 29;
            pub static EROFS : c_int = 30;
            pub static EMLINK : c_int = 31;
            pub static EPIPE : c_int = 32;
            pub static EDOM : c_int = 33;
            pub static ERANGE : c_int = 34;

            pub static EDEADLK: c_int = 35;
            pub static ENAMETOOLONG: c_int = 36;
            pub static ENOLCK: c_int = 37;
            pub static ENOSYS: c_int = 38;
            pub static ENOTEMPTY: c_int = 39;
            pub static ELOOP: c_int = 40;
            pub static EWOULDBLOCK: c_int = EAGAIN;
            pub static ENOMSG: c_int = 42;
            pub static EIDRM: c_int = 43;
            pub static ECHRNG: c_int = 44;
            pub static EL2NSYNC: c_int = 45;
            pub static EL3HLT: c_int = 46;
            pub static EL3RST: c_int = 47;
            pub static ELNRNG: c_int = 48;
            pub static EUNATCH: c_int = 49;
            pub static ENOCSI: c_int = 50;
            pub static EL2HLT: c_int = 51;
            pub static EBADE: c_int = 52;
            pub static EBADR: c_int = 53;
            pub static EXFULL: c_int = 54;
            pub static ENOANO: c_int = 55;
            pub static EBADRQC: c_int = 56;
            pub static EBADSLT: c_int = 57;

            pub static EDEADLOCK: c_int = EDEADLK;

            pub static EBFONT: c_int = 59;
            pub static ENOSTR: c_int = 60;
            pub static ENODATA: c_int = 61;
            pub static ETIME: c_int = 62;
            pub static ENOSR: c_int = 63;
            pub static ENONET: c_int = 64;
            pub static ENOPKG: c_int = 65;
            pub static EREMOTE: c_int = 66;
            pub static ENOLINK: c_int = 67;
            pub static EADV: c_int = 68;
            pub static ESRMNT: c_int = 69;
            pub static ECOMM: c_int = 70;
            pub static EPROTO: c_int = 71;
            pub static EMULTIHOP: c_int = 72;
            pub static EDOTDOT: c_int = 73;
            pub static EBADMSG: c_int = 74;
            pub static EOVERFLOW: c_int = 75;
            pub static ENOTUNIQ: c_int = 76;
            pub static EBADFD: c_int = 77;
            pub static EREMCHG: c_int = 78;
            pub static ELIBACC: c_int = 79;
            pub static ELIBBAD: c_int = 80;
            pub static ELIBSCN: c_int = 81;
            pub static ELIBMAX: c_int = 82;
            pub static ELIBEXEC: c_int = 83;
            pub static EILSEQ: c_int = 84;
            pub static ERESTART: c_int = 85;
            pub static ESTRPIPE: c_int = 86;
            pub static EUSERS: c_int = 87;
            pub static ENOTSOCK: c_int = 88;
            pub static EDESTADDRREQ: c_int = 89;
            pub static EMSGSIZE: c_int = 90;
            pub static EPROTOTYPE: c_int = 91;
            pub static ENOPROTOOPT: c_int = 92;
            pub static EPROTONOSUPPORT: c_int = 93;
            pub static ESOCKTNOSUPPORT: c_int = 94;
            pub static EOPNOTSUPP: c_int = 95;
            pub static EPFNOSUPPORT: c_int = 96;
            pub static EAFNOSUPPORT: c_int = 97;
            pub static EADDRINUSE: c_int = 98;
            pub static EADDRNOTAVAIL: c_int = 99;
            pub static ENETDOWN: c_int = 100;
            pub static ENETUNREACH: c_int = 101;
            pub static ENETRESET: c_int = 102;
            pub static ECONNABORTED: c_int = 103;
            pub static ECONNRESET: c_int = 104;
            pub static ENOBUFS: c_int = 105;
            pub static EISCONN: c_int = 106;
            pub static ENOTCONN: c_int = 107;
            pub static ESHUTDOWN: c_int = 108;
            pub static ETOOMANYREFS: c_int = 109;
            pub static ETIMEDOUT: c_int = 110;
            pub static ECONNREFUSED: c_int = 111;
            pub static EHOSTDOWN: c_int = 112;
            pub static EHOSTUNREACH: c_int = 113;
            pub static EALREADY: c_int = 114;
            pub static EINPROGRESS: c_int = 115;
            pub static ESTALE: c_int = 116;
            pub static EUCLEAN: c_int = 117;
            pub static ENOTNAM: c_int = 118;
            pub static ENAVAIL: c_int = 119;
            pub static EISNAM: c_int = 120;
            pub static EREMOTEIO: c_int = 121;
            pub static EDQUOT: c_int = 122;

            pub static ENOMEDIUM: c_int = 123;
            pub static EMEDIUMTYPE: c_int = 124;
            pub static ECANCELED: c_int = 125;
            pub static ENOKEY: c_int = 126;
            pub static EKEYEXPIRED: c_int = 127;
            pub static EKEYREVOKED: c_int = 128;
            pub static EKEYREJECTED: c_int = 129;

            pub static EOWNERDEAD: c_int = 130;
            pub static ENOTRECOVERABLE: c_int = 131;

            pub static ERFKILL: c_int = 132;

            pub static EHWPOISON: c_int = 133;
        }

        #[cfg(target_arch = "mips")]
        pub mod posix88 {
            use libc::types::os::arch::c95::c_int;
            use libc::types::common::c95::c_void;

            pub static O_RDONLY : c_int = 0;
            pub static O_WRONLY : c_int = 1;
            pub static O_RDWR : c_int = 2;
            pub static O_APPEND : c_int = 8;
            pub static O_CREAT : c_int = 256;
            pub static O_EXCL : c_int = 1024;
            pub static O_TRUNC : c_int = 512;
            pub static S_IFIFO : c_int = 4096;
            pub static S_IFCHR : c_int = 8192;
            pub static S_IFBLK : c_int = 24576;
            pub static S_IFDIR : c_int = 16384;
            pub static S_IFREG : c_int = 32768;
            pub static S_IFMT : c_int = 61440;
            pub static S_IEXEC : c_int = 64;
            pub static S_IWRITE : c_int = 128;
            pub static S_IREAD : c_int = 256;
            pub static S_IRWXU : c_int = 448;
            pub static S_IXUSR : c_int = 64;
            pub static S_IWUSR : c_int = 128;
            pub static S_IRUSR : c_int = 256;
            pub static F_OK : c_int = 0;
            pub static R_OK : c_int = 4;
            pub static W_OK : c_int = 2;
            pub static X_OK : c_int = 1;
            pub static STDIN_FILENO : c_int = 0;
            pub static STDOUT_FILENO : c_int = 1;
            pub static STDERR_FILENO : c_int = 2;
            pub static F_LOCK : c_int = 1;
            pub static F_TEST : c_int = 3;
            pub static F_TLOCK : c_int = 2;
            pub static F_ULOCK : c_int = 0;
            pub static SIGHUP : c_int = 1;
            pub static SIGINT : c_int = 2;
            pub static SIGQUIT : c_int = 3;
            pub static SIGILL : c_int = 4;
            pub static SIGABRT : c_int = 6;
            pub static SIGFPE : c_int = 8;
            pub static SIGKILL : c_int = 9;
            pub static SIGSEGV : c_int = 11;
            pub static SIGPIPE : c_int = 13;
            pub static SIGALRM : c_int = 14;
            pub static SIGTERM : c_int = 15;

            pub static PROT_NONE : c_int = 0;
            pub static PROT_READ : c_int = 1;
            pub static PROT_WRITE : c_int = 2;
            pub static PROT_EXEC : c_int = 4;

            pub static MAP_FILE : c_int = 0x0000;
            pub static MAP_SHARED : c_int = 0x0001;
            pub static MAP_PRIVATE : c_int = 0x0002;
            pub static MAP_FIXED : c_int = 0x0010;
            pub static MAP_ANON : c_int = 0x0020;

            pub static MAP_FAILED : *c_void = -1 as *c_void;

            pub static MCL_CURRENT : c_int = 0x0001;
            pub static MCL_FUTURE : c_int = 0x0002;

            pub static MS_ASYNC : c_int = 0x0001;
            pub static MS_INVALIDATE : c_int = 0x0002;
            pub static MS_SYNC : c_int = 0x0004;

            pub static EPERM : c_int = 1;
            pub static ENOENT : c_int = 2;
            pub static ESRCH : c_int = 3;
            pub static EINTR : c_int = 4;
            pub static EIO : c_int = 5;
            pub static ENXIO : c_int = 6;
            pub static E2BIG : c_int = 7;
            pub static ENOEXEC : c_int = 8;
            pub static EBADF : c_int = 9;
            pub static ECHILD : c_int = 10;
            pub static EAGAIN : c_int = 11;
            pub static ENOMEM : c_int = 12;
            pub static EACCES : c_int = 13;
            pub static EFAULT : c_int = 14;
            pub static ENOTBLK : c_int = 15;
            pub static EBUSY : c_int = 16;
            pub static EEXIST : c_int = 17;
            pub static EXDEV : c_int = 18;
            pub static ENODEV : c_int = 19;
            pub static ENOTDIR : c_int = 20;
            pub static EISDIR : c_int = 21;
            pub static EINVAL : c_int = 22;
            pub static ENFILE : c_int = 23;
            pub static EMFILE : c_int = 24;
            pub static ENOTTY : c_int = 25;
            pub static ETXTBSY : c_int = 26;
            pub static EFBIG : c_int = 27;
            pub static ENOSPC : c_int = 28;
            pub static ESPIPE : c_int = 29;
            pub static EROFS : c_int = 30;
            pub static EMLINK : c_int = 31;
            pub static EPIPE : c_int = 32;
            pub static EDOM : c_int = 33;
            pub static ERANGE : c_int = 34;

            pub static ENOMSG: c_int = 35;
            pub static EIDRM: c_int = 36;
            pub static ECHRNG: c_int = 37;
            pub static EL2NSYNC: c_int = 38;
            pub static EL3HLT: c_int = 39;
            pub static EL3RST: c_int = 40;
            pub static ELNRNG: c_int = 41;
            pub static EUNATCH: c_int = 42;
            pub static ENOCSI: c_int = 43;
            pub static EL2HLT: c_int = 44;
            pub static EDEADLK: c_int = 45;
            pub static ENOLCK: c_int = 46;
            pub static EBADE: c_int = 50;
            pub static EBADR: c_int = 51;
            pub static EXFULL: c_int = 52;
            pub static ENOANO: c_int = 53;
            pub static EBADRQC: c_int = 54;
            pub static EBADSLT: c_int = 55;
            pub static EDEADLOCK: c_int = 56;
            pub static EBFONT: c_int = 59;
            pub static ENOSTR: c_int = 60;
            pub static ENODATA: c_int = 61;
            pub static ETIME: c_int = 62;
            pub static ENOSR: c_int = 63;
            pub static ENONET: c_int = 64;
            pub static ENOPKG: c_int = 65;
            pub static EREMOTE: c_int = 66;
            pub static ENOLINK: c_int = 67;
            pub static EADV: c_int = 68;
            pub static ESRMNT: c_int = 69;
            pub static ECOMM: c_int = 70;
            pub static EPROTO: c_int = 71;
            pub static EDOTDOT: c_int = 73;
            pub static EMULTIHOP: c_int = 74;
            pub static EBADMSG: c_int = 77;
            pub static ENAMETOOLONG: c_int = 78;
            pub static EOVERFLOW: c_int = 79;
            pub static ENOTUNIQ: c_int = 80;
            pub static EBADFD: c_int = 81;
            pub static EREMCHG: c_int = 82;
            pub static ELIBACC: c_int = 83;
            pub static ELIBBAD: c_int = 84;
            pub static ELIBSCN: c_int = 95;
            pub static ELIBMAX: c_int = 86;
            pub static ELIBEXEC: c_int = 87;
            pub static EILSEQ: c_int = 88;
            pub static ENOSYS: c_int = 89;
            pub static ELOOP: c_int = 90;
            pub static ERESTART: c_int = 91;
            pub static ESTRPIPE: c_int = 92;
            pub static ENOTEMPTY: c_int = 93;
            pub static EUSERS: c_int = 94;
            pub static ENOTSOCK: c_int = 95;
            pub static EDESTADDRREQ: c_int = 96;
            pub static EMSGSIZE: c_int = 97;
            pub static EPROTOTYPE: c_int = 98;
            pub static ENOPROTOOPT: c_int = 99;
            pub static EPROTONOSUPPORT: c_int = 120;
            pub static ESOCKTNOSUPPORT: c_int = 121;
            pub static EOPNOTSUPP: c_int = 122;
            pub static EPFNOSUPPORT: c_int = 123;
            pub static EAFNOSUPPORT: c_int = 124;
            pub static EADDRINUSE: c_int = 125;
            pub static EADDRNOTAVAIL: c_int = 126;
            pub static ENETDOWN: c_int = 127;
            pub static ENETUNREACH: c_int = 128;
            pub static ENETRESET: c_int = 129;
            pub static ECONNABORTED: c_int = 130;
            pub static ECONNRESET: c_int = 131;
            pub static ENOBUFS: c_int = 132;
            pub static EISCONN: c_int = 133;
            pub static ENOTCONN: c_int = 134;
            pub static EUCLEAN: c_int = 135;
            pub static ENOTNAM: c_int = 137;
            pub static ENAVAIL: c_int = 138;
            pub static EISNAM: c_int = 139;
            pub static EREMOTEIO: c_int = 140;
            pub static ESHUTDOWN: c_int = 143;
            pub static ETOOMANYREFS: c_int = 144;
            pub static ETIMEDOUT: c_int = 145;
            pub static ECONNREFUSED: c_int = 146;
            pub static EHOSTDOWN: c_int = 147;
            pub static EHOSTUNREACH: c_int = 148;
            pub static EWOULDBLOCK: c_int = EAGAIN;
            pub static EALREADY: c_int = 149;
            pub static EINPROGRESS: c_int = 150;
            pub static ESTALE: c_int = 151;
            pub static ECANCELED: c_int = 158;

            pub static ENOMEDIUM: c_int = 159;
            pub static EMEDIUMTYPE: c_int = 160;
            pub static ENOKEY: c_int = 161;
            pub static EKEYEXPIRED: c_int = 162;
            pub static EKEYREVOKED: c_int = 163;
            pub static EKEYREJECTED: c_int = 164;

            pub static EOWNERDEAD: c_int = 165;
            pub static ENOTRECOVERABLE: c_int = 166;

            pub static ERFKILL: c_int = 167;

            pub static EHWPOISON: c_int = 168;

            pub static EDQUOT: c_int = 1133;
        }
        pub mod posix01 {
            use libc::types::os::arch::c95::c_int;

            pub static SIGTRAP : c_int = 5;

            pub static GLOB_ERR      : c_int = 1 << 0;
            pub static GLOB_MARK     : c_int = 1 << 1;
            pub static GLOB_NOSORT   : c_int = 1 << 2;
            pub static GLOB_DOOFFS   : c_int = 1 << 3;
            pub static GLOB_NOCHECK  : c_int = 1 << 4;
            pub static GLOB_APPEND   : c_int = 1 << 5;
            pub static GLOB_NOESCAPE : c_int = 1 << 6;

            pub static GLOB_NOSPACE  : c_int = 1;
            pub static GLOB_ABORTED  : c_int = 2;
            pub static GLOB_NOMATCH  : c_int = 3;

            pub static POSIX_MADV_NORMAL : c_int = 0;
            pub static POSIX_MADV_RANDOM : c_int = 1;
            pub static POSIX_MADV_SEQUENTIAL : c_int = 2;
            pub static POSIX_MADV_WILLNEED : c_int = 3;
            pub static POSIX_MADV_DONTNEED : c_int = 4;

            pub static _SC_MQ_PRIO_MAX : c_int = 28;
            pub static _SC_IOV_MAX : c_int = 60;
            pub static _SC_GETGR_R_SIZE_MAX : c_int = 69;
            pub static _SC_GETPW_R_SIZE_MAX : c_int = 70;
            pub static _SC_LOGIN_NAME_MAX : c_int = 71;
            pub static _SC_TTY_NAME_MAX : c_int = 72;
            pub static _SC_THREADS : c_int = 67;
            pub static _SC_THREAD_SAFE_FUNCTIONS : c_int = 68;
            pub static _SC_THREAD_DESTRUCTOR_ITERATIONS : c_int = 73;
            pub static _SC_THREAD_KEYS_MAX : c_int = 74;
            pub static _SC_THREAD_STACK_MIN : c_int = 75;
            pub static _SC_THREAD_THREADS_MAX : c_int = 76;
            pub static _SC_THREAD_ATTR_STACKADDR : c_int = 77;
            pub static _SC_THREAD_ATTR_STACKSIZE : c_int = 78;
            pub static _SC_THREAD_PRIORITY_SCHEDULING : c_int = 79;
            pub static _SC_THREAD_PRIO_INHERIT : c_int = 80;
            pub static _SC_THREAD_PRIO_PROTECT : c_int = 81;
            pub static _SC_THREAD_PROCESS_SHARED : c_int = 82;
            pub static _SC_ATEXIT_MAX : c_int = 87;
            pub static _SC_XOPEN_VERSION : c_int = 89;
            pub static _SC_XOPEN_XCU_VERSION : c_int = 90;
            pub static _SC_XOPEN_UNIX : c_int = 91;
            pub static _SC_XOPEN_CRYPT : c_int = 92;
            pub static _SC_XOPEN_ENH_I18N : c_int = 93;
            pub static _SC_XOPEN_SHM : c_int = 94;
            pub static _SC_XOPEN_LEGACY : c_int = 129;
            pub static _SC_XOPEN_REALTIME : c_int = 130;
            pub static _SC_XOPEN_REALTIME_THREADS : c_int = 131;
        }
        pub mod posix08 {
        }
        pub mod bsd44 {
            use libc::types::os::arch::c95::c_int;

            pub static MADV_NORMAL : c_int = 0;
            pub static MADV_RANDOM : c_int = 1;
            pub static MADV_SEQUENTIAL : c_int = 2;
            pub static MADV_WILLNEED : c_int = 3;
            pub static MADV_DONTNEED : c_int = 4;
            pub static MADV_REMOVE : c_int = 9;
            pub static MADV_DONTFORK : c_int = 10;
            pub static MADV_DOFORK : c_int = 11;
            pub static MADV_MERGEABLE : c_int = 12;
            pub static MADV_UNMERGEABLE : c_int = 13;
            pub static MADV_HWPOISON : c_int = 100;
        }
        #[cfg(target_arch = "x86")]
        #[cfg(target_arch = "x86_64")]
        #[cfg(target_arch = "arm")]
        pub mod extra {
            use libc::types::os::arch::c95::c_int;

            pub static O_RSYNC : c_int = 1052672;
            pub static O_DSYNC : c_int = 4096;
            pub static O_SYNC : c_int = 1052672;

            pub static PROT_GROWSDOWN : c_int = 0x010000000;
            pub static PROT_GROWSUP : c_int = 0x020000000;

            pub static MAP_TYPE : c_int = 0x000f;
            pub static MAP_ANONONYMOUS : c_int = 0x0020;
            pub static MAP_32BIT : c_int = 0x0040;
            pub static MAP_GROWSDOWN : c_int = 0x0100;
            pub static MAP_DENYWRITE : c_int = 0x0800;
            pub static MAP_EXECUTABLE : c_int = 0x01000;
            pub static MAP_LOCKED : c_int = 0x02000;
            pub static MAP_NONRESERVE : c_int = 0x04000;
            pub static MAP_POPULATE : c_int = 0x08000;
            pub static MAP_NONBLOCK : c_int = 0x010000;
            pub static MAP_STACK : c_int = 0x020000;
        }
        #[cfg(target_arch = "mips")]
        pub mod extra {
            use libc::types::os::arch::c95::c_int;

            pub static O_RSYNC : c_int = 16400;
            pub static O_DSYNC : c_int = 16;
            pub static O_SYNC : c_int = 16400;

            pub static PROT_GROWSDOWN : c_int = 0x010000000;
            pub static PROT_GROWSUP : c_int = 0x020000000;

            pub static MAP_TYPE : c_int = 0x000f;
            pub static MAP_ANONONYMOUS : c_int = 0x0020;
            pub static MAP_32BIT : c_int = 0x0040;
            pub static MAP_GROWSDOWN : c_int = 0x0100;
            pub static MAP_DENYWRITE : c_int = 0x0800;
            pub static MAP_EXECUTABLE : c_int = 0x01000;
            pub static MAP_LOCKED : c_int = 0x02000;
            pub static MAP_NONRESERVE : c_int = 0x04000;
            pub static MAP_POPULATE : c_int = 0x08000;
            pub static MAP_NONBLOCK : c_int = 0x010000;
            pub static MAP_STACK : c_int = 0x020000;
        }
        #[cfg(target_os = "linux")]
        pub mod sysconf {
            use libc::types::os::arch::c95::c_int;

            pub static _SC_ARG_MAX : c_int = 0;
            pub static _SC_CHILD_MAX : c_int = 1;
            pub static _SC_CLK_TCK : c_int = 2;
            pub static _SC_NGROUPS_MAX : c_int = 3;
            pub static _SC_OPEN_MAX : c_int = 4;
            pub static _SC_STREAM_MAX : c_int = 5;
            pub static _SC_TZNAME_MAX : c_int = 6;
            pub static _SC_JOB_CONTROL : c_int = 7;
            pub static _SC_SAVED_IDS : c_int = 8;
            pub static _SC_REALTIME_SIGNALS : c_int = 9;
            pub static _SC_PRIORITY_SCHEDULING : c_int = 10;
            pub static _SC_TIMERS : c_int = 11;
            pub static _SC_ASYNCHRONOUS_IO : c_int = 12;
            pub static _SC_PRIORITIZED_IO : c_int = 13;
            pub static _SC_SYNCHRONIZED_IO : c_int = 14;
            pub static _SC_FSYNC : c_int = 15;
            pub static _SC_MAPPED_FILES : c_int = 16;
            pub static _SC_MEMLOCK : c_int = 17;
            pub static _SC_MEMLOCK_RANGE : c_int = 18;
            pub static _SC_MEMORY_PROTECTION : c_int = 19;
            pub static _SC_MESSAGE_PASSING : c_int = 20;
            pub static _SC_SEMAPHORES : c_int = 21;
            pub static _SC_SHARED_MEMORY_OBJECTS : c_int = 22;
            pub static _SC_AIO_LISTIO_MAX : c_int = 23;
            pub static _SC_AIO_MAX : c_int = 24;
            pub static _SC_AIO_PRIO_DELTA_MAX : c_int = 25;
            pub static _SC_DELAYTIMER_MAX : c_int = 26;
            pub static _SC_MQ_OPEN_MAX : c_int = 27;
            pub static _SC_VERSION : c_int = 29;
            pub static _SC_PAGESIZE : c_int = 30;
            pub static _SC_RTSIG_MAX : c_int = 31;
            pub static _SC_SEM_NSEMS_MAX : c_int = 32;
            pub static _SC_SEM_VALUE_MAX : c_int = 33;
            pub static _SC_SIGQUEUE_MAX : c_int = 34;
            pub static _SC_TIMER_MAX : c_int = 35;
            pub static _SC_BC_BASE_MAX : c_int = 36;
            pub static _SC_BC_DIM_MAX : c_int = 37;
            pub static _SC_BC_SCALE_MAX : c_int = 38;
            pub static _SC_BC_STRING_MAX : c_int = 39;
            pub static _SC_COLL_WEIGHTS_MAX : c_int = 40;
            pub static _SC_EXPR_NEST_MAX : c_int = 42;
            pub static _SC_LINE_MAX : c_int = 43;
            pub static _SC_RE_DUP_MAX : c_int = 44;
            pub static _SC_2_VERSION : c_int = 46;
            pub static _SC_2_C_BIND : c_int = 47;
            pub static _SC_2_C_DEV : c_int = 48;
            pub static _SC_2_FORT_DEV : c_int = 49;
            pub static _SC_2_FORT_RUN : c_int = 50;
            pub static _SC_2_SW_DEV : c_int = 51;
            pub static _SC_2_LOCALEDEF : c_int = 52;
            pub static _SC_2_CHAR_TERM : c_int = 95;
            pub static _SC_2_C_VERSION : c_int = 96;
            pub static _SC_2_UPE : c_int = 97;
            pub static _SC_XBS5_ILP32_OFF32 : c_int = 125;
            pub static _SC_XBS5_ILP32_OFFBIG : c_int = 126;
            pub static _SC_XBS5_LPBIG_OFFBIG : c_int = 128;
        }
        #[cfg(target_os = "android")]
        pub mod sysconf {
            use libc::types::os::arch::c95::c_int;

            pub static _SC_ARG_MAX : c_int = 0;
            pub static _SC_BC_BASE_MAX : c_int = 1;
            pub static _SC_BC_DIM_MAX : c_int = 2;
            pub static _SC_BC_SCALE_MAX : c_int = 3;
            pub static _SC_BC_STRING_MAX : c_int = 4;
            pub static _SC_CHILD_MAX : c_int = 5;
            pub static _SC_CLK_TCK : c_int = 6;
            pub static _SC_COLL_WEIGHTS_MAX : c_int = 7;
            pub static _SC_EXPR_NEST_MAX : c_int = 8;
            pub static _SC_LINE_MAX : c_int = 9;
            pub static _SC_NGROUPS_MAX : c_int = 10;
            pub static _SC_OPEN_MAX : c_int = 11;
            pub static _SC_2_C_BIND : c_int = 13;
            pub static _SC_2_C_DEV : c_int = 14;
            pub static _SC_2_C_VERSION : c_int = 15;
            pub static _SC_2_CHAR_TERM : c_int = 16;
            pub static _SC_2_FORT_DEV : c_int = 17;
            pub static _SC_2_FORT_RUN : c_int = 18;
            pub static _SC_2_LOCALEDEF : c_int = 19;
            pub static _SC_2_SW_DEV : c_int = 20;
            pub static _SC_2_UPE : c_int = 21;
            pub static _SC_2_VERSION : c_int = 22;
            pub static _SC_JOB_CONTROL : c_int = 23;
            pub static _SC_SAVED_IDS : c_int = 24;
            pub static _SC_VERSION : c_int = 25;
            pub static _SC_RE_DUP_MAX : c_int = 26;
            pub static _SC_STREAM_MAX : c_int = 27;
            pub static _SC_TZNAME_MAX : c_int = 28;
            pub static _SC_PAGESIZE : c_int = 39;
        }
    }

    #[cfg(target_os = "freebsd")]
    pub mod os {
        pub mod c95 {
            use libc::types::os::arch::c95::{c_int, c_uint};

            pub static EXIT_FAILURE : c_int = 1;
            pub static EXIT_SUCCESS : c_int = 0;
            pub static RAND_MAX : c_int = 2147483647;
            pub static EOF : c_int = -1;
            pub static SEEK_SET : c_int = 0;
            pub static SEEK_CUR : c_int = 1;
            pub static SEEK_END : c_int = 2;
            pub static _IOFBF : c_int = 0;
            pub static _IONBF : c_int = 2;
            pub static _IOLBF : c_int = 1;
            pub static BUFSIZ : c_uint = 1024_u32;
            pub static FOPEN_MAX : c_uint = 20_u32;
            pub static FILENAME_MAX : c_uint = 1024_u32;
            pub static L_tmpnam : c_uint = 1024_u32;
            pub static TMP_MAX : c_uint = 308915776_u32;
        }
        pub mod c99 {
        }
        pub mod posix88 {
            use libc::types::common::c95::c_void;
            use libc::types::os::arch::c95::c_int;

            pub static O_RDONLY : c_int = 0;
            pub static O_WRONLY : c_int = 1;
            pub static O_RDWR : c_int = 2;
            pub static O_APPEND : c_int = 8;
            pub static O_CREAT : c_int = 512;
            pub static O_EXCL : c_int = 2048;
            pub static O_TRUNC : c_int = 1024;
            pub static S_IFIFO : c_int = 4096;
            pub static S_IFCHR : c_int = 8192;
            pub static S_IFBLK : c_int = 24576;
            pub static S_IFDIR : c_int = 16384;
            pub static S_IFREG : c_int = 32768;
            pub static S_IFMT : c_int = 61440;
            pub static S_IEXEC : c_int = 64;
            pub static S_IWRITE : c_int = 128;
            pub static S_IREAD : c_int = 256;
            pub static S_IRWXU : c_int = 448;
            pub static S_IXUSR : c_int = 64;
            pub static S_IWUSR : c_int = 128;
            pub static S_IRUSR : c_int = 256;
            pub static F_OK : c_int = 0;
            pub static R_OK : c_int = 4;
            pub static W_OK : c_int = 2;
            pub static X_OK : c_int = 1;
            pub static STDIN_FILENO : c_int = 0;
            pub static STDOUT_FILENO : c_int = 1;
            pub static STDERR_FILENO : c_int = 2;
            pub static F_LOCK : c_int = 1;
            pub static F_TEST : c_int = 3;
            pub static F_TLOCK : c_int = 2;
            pub static F_ULOCK : c_int = 0;
            pub static SIGHUP : c_int = 1;
            pub static SIGINT : c_int = 2;
            pub static SIGQUIT : c_int = 3;
            pub static SIGILL : c_int = 4;
            pub static SIGABRT : c_int = 6;
            pub static SIGFPE : c_int = 8;
            pub static SIGKILL : c_int = 9;
            pub static SIGSEGV : c_int = 11;
            pub static SIGPIPE : c_int = 13;
            pub static SIGALRM : c_int = 14;
            pub static SIGTERM : c_int = 15;

            pub static PROT_NONE : c_int = 0;
            pub static PROT_READ : c_int = 1;
            pub static PROT_WRITE : c_int = 2;
            pub static PROT_EXEC : c_int = 4;

            pub static MAP_FILE : c_int = 0x0000;
            pub static MAP_SHARED : c_int = 0x0001;
            pub static MAP_PRIVATE : c_int = 0x0002;
            pub static MAP_FIXED : c_int = 0x0010;
            pub static MAP_ANON : c_int = 0x1000;

            pub static MAP_FAILED : *c_void = -1 as *c_void;

            pub static MCL_CURRENT : c_int = 0x0001;
            pub static MCL_FUTURE : c_int = 0x0002;

            pub static MS_SYNC : c_int = 0x0000;
            pub static MS_ASYNC : c_int = 0x0001;
            pub static MS_INVALIDATE : c_int = 0x0002;

            pub static EPERM : c_int = 1;
            pub static ENOENT : c_int = 2;
            pub static ESRCH : c_int = 3;
            pub static EINTR : c_int = 4;
            pub static EIO : c_int = 5;
            pub static ENXIO : c_int = 6;
            pub static E2BIG : c_int = 7;
            pub static ENOEXEC : c_int = 8;
            pub static EBADF : c_int = 9;
            pub static ECHILD : c_int = 10;
            pub static EDEADLK : c_int = 11;
            pub static ENOMEM : c_int = 12;
            pub static EACCES : c_int = 13;
            pub static EFAULT : c_int = 14;
            pub static ENOTBLK : c_int = 15;
            pub static EBUSY : c_int = 16;
            pub static EEXIST : c_int = 17;
            pub static EXDEV : c_int = 18;
            pub static ENODEV : c_int = 19;
            pub static ENOTDIR : c_int = 20;
            pub static EISDIR : c_int = 21;
            pub static EINVAL : c_int = 22;
            pub static ENFILE : c_int = 23;
            pub static EMFILE : c_int = 24;
            pub static ENOTTY : c_int = 25;
            pub static ETXTBSY : c_int = 26;
            pub static EFBIG : c_int = 27;
            pub static ENOSPC : c_int = 28;
            pub static ESPIPE : c_int = 29;
            pub static EROFS : c_int = 30;
            pub static EMLINK : c_int = 31;
            pub static EPIPE : c_int = 32;
            pub static EDOM : c_int = 33;
            pub static ERANGE : c_int = 34;
            pub static EAGAIN : c_int = 35;
            pub static EWOULDBLOCK : c_int = 35;
            pub static EINPROGRESS : c_int = 36;
            pub static EALREADY : c_int = 37;
            pub static ENOTSOCK : c_int = 38;
            pub static EDESTADDRREQ : c_int = 39;
            pub static EMSGSIZE : c_int = 40;
            pub static EPROTOTYPE : c_int = 41;
            pub static ENOPROTOOPT : c_int = 42;
            pub static EPROTONOSUPPORT : c_int = 43;
            pub static ESOCKTNOSUPPORT : c_int = 44;
            pub static EOPNOTSUPP : c_int = 45;
            pub static EPFNOSUPPORT : c_int = 46;
            pub static EAFNOSUPPORT : c_int = 47;
            pub static EADDRINUSE : c_int = 48;
            pub static EADDRNOTAVAIL : c_int = 49;
            pub static ENETDOWN : c_int = 50;
            pub static ENETUNREACH : c_int = 51;
            pub static ENETRESET : c_int = 52;
            pub static ECONNABORTED : c_int = 53;
            pub static ECONNRESET : c_int = 54;
            pub static ENOBUFS : c_int = 55;
            pub static EISCONN : c_int = 56;
            pub static ENOTCONN : c_int = 57;
            pub static ESHUTDOWN : c_int = 58;
            pub static ETOOMANYREFS : c_int = 59;
            pub static ETIMEDOUT : c_int = 60;
            pub static ECONNREFUSED : c_int = 61;
            pub static ELOOP : c_int = 62;
            pub static ENAMETOOLONG : c_int = 63;
            pub static EHOSTDOWN : c_int = 64;
            pub static EHOSTUNREACH : c_int = 65;
            pub static ENOTEMPTY : c_int = 66;
            pub static EPROCLIM : c_int = 67;
            pub static EUSERS : c_int = 68;
            pub static EDQUOT : c_int = 69;
            pub static ESTALE : c_int = 70;
            pub static EREMOTE : c_int = 71;
            pub static EBADRPC : c_int = 72;
            pub static ERPCMISMATCH : c_int = 73;
            pub static EPROGUNAVAIL : c_int = 74;
            pub static EPROGMISMATCH : c_int = 75;
            pub static EPROCUNAVAIL : c_int = 76;
            pub static ENOLCK : c_int = 77;
            pub static ENOSYS : c_int = 78;
            pub static EFTYPE : c_int = 79;
            pub static EAUTH : c_int = 80;
            pub static ENEEDAUTH : c_int = 81;
            pub static EIDRM : c_int = 82;
            pub static ENOMSG : c_int = 83;
            pub static EOVERFLOW : c_int = 84;
            pub static ECANCELED : c_int = 85;
            pub static EILSEQ : c_int = 86;
            pub static ENOATTR : c_int = 87;
            pub static EDOOFUS : c_int = 88;
            pub static EBADMSG : c_int = 89;
            pub static EMULTIHOP : c_int = 90;
            pub static ENOLINK : c_int = 91;
            pub static EPROTO : c_int = 92;
            pub static ENOMEDIUM : c_int = 93;
            pub static EUNUSED94 : c_int = 94;
            pub static EUNUSED95 : c_int = 95;
            pub static EUNUSED96 : c_int = 96;
            pub static EUNUSED97 : c_int = 97;
            pub static EUNUSED98 : c_int = 98;
            pub static EASYNC : c_int = 99;
            pub static ELAST : c_int = 99;
        }
        pub mod posix01 {
            use libc::types::os::arch::c95::c_int;

            pub static SIGTRAP : c_int = 5;

            pub static GLOB_APPEND   : c_int = 0x0001;
            pub static GLOB_DOOFFS   : c_int = 0x0002;
            pub static GLOB_ERR      : c_int = 0x0004;
            pub static GLOB_MARK     : c_int = 0x0008;
            pub static GLOB_NOCHECK  : c_int = 0x0010;
            pub static GLOB_NOSORT   : c_int = 0x0020;
            pub static GLOB_NOESCAPE : c_int = 0x2000;

            pub static GLOB_NOSPACE  : c_int = -1;
            pub static GLOB_ABORTED  : c_int = -2;
            pub static GLOB_NOMATCH  : c_int = -3;

            pub static POSIX_MADV_NORMAL : c_int = 0;
            pub static POSIX_MADV_RANDOM : c_int = 1;
            pub static POSIX_MADV_SEQUENTIAL : c_int = 2;
            pub static POSIX_MADV_WILLNEED : c_int = 3;
            pub static POSIX_MADV_DONTNEED : c_int = 4;

            pub static _SC_IOV_MAX : c_int = 56;
            pub static _SC_GETGR_R_SIZE_MAX : c_int = 70;
            pub static _SC_GETPW_R_SIZE_MAX : c_int = 71;
            pub static _SC_LOGIN_NAME_MAX : c_int = 73;
            pub static _SC_MQ_PRIO_MAX : c_int = 75;
            pub static _SC_THREAD_ATTR_STACKADDR : c_int = 82;
            pub static _SC_THREAD_ATTR_STACKSIZE : c_int = 83;
            pub static _SC_THREAD_DESTRUCTOR_ITERATIONS : c_int = 85;
            pub static _SC_THREAD_KEYS_MAX : c_int = 86;
            pub static _SC_THREAD_PRIO_INHERIT : c_int = 87;
            pub static _SC_THREAD_PRIO_PROTECT : c_int = 88;
            pub static _SC_THREAD_PRIORITY_SCHEDULING : c_int = 89;
            pub static _SC_THREAD_PROCESS_SHARED : c_int = 90;
            pub static _SC_THREAD_SAFE_FUNCTIONS : c_int = 91;
            pub static _SC_THREAD_STACK_MIN : c_int = 93;
            pub static _SC_THREAD_THREADS_MAX : c_int = 94;
            pub static _SC_THREADS : c_int = 96;
            pub static _SC_TTY_NAME_MAX : c_int = 101;
            pub static _SC_ATEXIT_MAX : c_int = 107;
            pub static _SC_XOPEN_CRYPT : c_int = 108;
            pub static _SC_XOPEN_ENH_I18N : c_int = 109;
            pub static _SC_XOPEN_LEGACY : c_int = 110;
            pub static _SC_XOPEN_REALTIME : c_int = 111;
            pub static _SC_XOPEN_REALTIME_THREADS : c_int = 112;
            pub static _SC_XOPEN_SHM : c_int = 113;
            pub static _SC_XOPEN_UNIX : c_int = 115;
            pub static _SC_XOPEN_VERSION : c_int = 116;
            pub static _SC_XOPEN_XCU_VERSION : c_int = 117;
        }
        pub mod posix08 {
        }
        pub mod bsd44 {
            use libc::types::os::arch::c95::c_int;

            pub static MADV_NORMAL : c_int = 0;
            pub static MADV_RANDOM : c_int = 1;
            pub static MADV_SEQUENTIAL : c_int = 2;
            pub static MADV_WILLNEED : c_int = 3;
            pub static MADV_DONTNEED : c_int = 4;
            pub static MADV_FREE : c_int = 5;
            pub static MADV_NOSYNC : c_int = 6;
            pub static MADV_AUTOSYNC : c_int = 7;
            pub static MADV_NOCORE : c_int = 8;
            pub static MADV_CORE : c_int = 9;
            pub static MADV_PROTECT : c_int = 10;

            pub static MINCORE_INCORE : c_int =  0x1;
            pub static MINCORE_REFERENCED : c_int = 0x2;
            pub static MINCORE_MODIFIED : c_int = 0x4;
            pub static MINCORE_REFERENCED_OTHER : c_int = 0x8;
            pub static MINCORE_MODIFIED_OTHER : c_int = 0x10;
            pub static MINCORE_SUPER : c_int = 0x20;
        }
        pub mod extra {
            use libc::types::os::arch::c95::c_int;

            pub static O_SYNC : c_int = 128;
            pub static CTL_KERN: c_int = 1;
            pub static KERN_PROC: c_int = 14;
            pub static KERN_PROC_PATHNAME: c_int = 12;

            pub static MAP_COPY : c_int = 0x0002;
            pub static MAP_RENAME : c_int = 0x0020;
            pub static MAP_NORESERVE : c_int = 0x0040;
            pub static MAP_HASSEMAPHORE : c_int = 0x0200;
            pub static MAP_STACK : c_int = 0x0400;
            pub static MAP_NOSYNC : c_int = 0x0800;
            pub static MAP_NOCORE : c_int = 0x020000;
        }
        pub mod sysconf {
            use libc::types::os::arch::c95::c_int;

            pub static _SC_ARG_MAX : c_int = 1;
            pub static _SC_CHILD_MAX : c_int = 2;
            pub static _SC_CLK_TCK : c_int = 3;
            pub static _SC_NGROUPS_MAX : c_int = 4;
            pub static _SC_OPEN_MAX : c_int = 5;
            pub static _SC_JOB_CONTROL : c_int = 6;
            pub static _SC_SAVED_IDS : c_int = 7;
            pub static _SC_VERSION : c_int = 8;
            pub static _SC_BC_BASE_MAX : c_int = 9;
            pub static _SC_BC_DIM_MAX : c_int = 10;
            pub static _SC_BC_SCALE_MAX : c_int = 11;
            pub static _SC_BC_STRING_MAX : c_int = 12;
            pub static _SC_COLL_WEIGHTS_MAX : c_int = 13;
            pub static _SC_EXPR_NEST_MAX : c_int = 14;
            pub static _SC_LINE_MAX : c_int = 15;
            pub static _SC_RE_DUP_MAX : c_int = 16;
            pub static _SC_2_VERSION : c_int = 17;
            pub static _SC_2_C_BIND : c_int = 18;
            pub static _SC_2_C_DEV : c_int = 19;
            pub static _SC_2_CHAR_TERM : c_int = 20;
            pub static _SC_2_FORT_DEV : c_int = 21;
            pub static _SC_2_FORT_RUN : c_int = 22;
            pub static _SC_2_LOCALEDEF : c_int = 23;
            pub static _SC_2_SW_DEV : c_int = 24;
            pub static _SC_2_UPE : c_int = 25;
            pub static _SC_STREAM_MAX : c_int = 26;
            pub static _SC_TZNAME_MAX : c_int = 27;
            pub static _SC_ASYNCHRONOUS_IO : c_int = 28;
            pub static _SC_MAPPED_FILES : c_int = 29;
            pub static _SC_MEMLOCK : c_int = 30;
            pub static _SC_MEMLOCK_RANGE : c_int = 31;
            pub static _SC_MEMORY_PROTECTION : c_int = 32;
            pub static _SC_MESSAGE_PASSING : c_int = 33;
            pub static _SC_PRIORITIZED_IO : c_int = 34;
            pub static _SC_PRIORITY_SCHEDULING : c_int = 35;
            pub static _SC_REALTIME_SIGNALS : c_int = 36;
            pub static _SC_SEMAPHORES : c_int = 37;
            pub static _SC_FSYNC : c_int = 38;
            pub static _SC_SHARED_MEMORY_OBJECTS : c_int = 39;
            pub static _SC_SYNCHRONIZED_IO : c_int = 40;
            pub static _SC_TIMERS : c_int = 41;
            pub static _SC_AIO_LISTIO_MAX : c_int = 42;
            pub static _SC_AIO_MAX : c_int = 43;
            pub static _SC_AIO_PRIO_DELTA_MAX : c_int = 44;
            pub static _SC_DELAYTIMER_MAX : c_int = 45;
            pub static _SC_MQ_OPEN_MAX : c_int = 46;
            pub static _SC_PAGESIZE : c_int = 47;
            pub static _SC_RTSIG_MAX : c_int = 48;
            pub static _SC_SEM_NSEMS_MAX : c_int = 49;
            pub static _SC_SEM_VALUE_MAX : c_int = 50;
            pub static _SC_SIGQUEUE_MAX : c_int = 51;
            pub static _SC_TIMER_MAX : c_int = 52;
        }
    }

    #[cfg(target_os = "macos")]
    pub mod os {
        pub mod c95 {
            use libc::types::os::arch::c95::{c_int, c_uint};

            pub static EXIT_FAILURE : c_int = 1;
            pub static EXIT_SUCCESS : c_int = 0;
            pub static RAND_MAX : c_int = 2147483647;
            pub static EOF : c_int = -1;
            pub static SEEK_SET : c_int = 0;
            pub static SEEK_CUR : c_int = 1;
            pub static SEEK_END : c_int = 2;
            pub static _IOFBF : c_int = 0;
            pub static _IONBF : c_int = 2;
            pub static _IOLBF : c_int = 1;
            pub static BUFSIZ : c_uint = 1024_u32;
            pub static FOPEN_MAX : c_uint = 20_u32;
            pub static FILENAME_MAX : c_uint = 1024_u32;
            pub static L_tmpnam : c_uint = 1024_u32;
            pub static TMP_MAX : c_uint = 308915776_u32;
        }
        pub mod c99 {
        }
        pub mod posix88 {
            use libc::types::common::c95::c_void;
            use libc::types::os::arch::c95::c_int;

            pub static O_RDONLY : c_int = 0;
            pub static O_WRONLY : c_int = 1;
            pub static O_RDWR : c_int = 2;
            pub static O_APPEND : c_int = 8;
            pub static O_CREAT : c_int = 512;
            pub static O_EXCL : c_int = 2048;
            pub static O_TRUNC : c_int = 1024;
            pub static S_IFIFO : c_int = 4096;
            pub static S_IFCHR : c_int = 8192;
            pub static S_IFBLK : c_int = 24576;
            pub static S_IFDIR : c_int = 16384;
            pub static S_IFREG : c_int = 32768;
            pub static S_IFMT : c_int = 61440;
            pub static S_IEXEC : c_int = 64;
            pub static S_IWRITE : c_int = 128;
            pub static S_IREAD : c_int = 256;
            pub static S_IRWXU : c_int = 448;
            pub static S_IXUSR : c_int = 64;
            pub static S_IWUSR : c_int = 128;
            pub static S_IRUSR : c_int = 256;
            pub static F_OK : c_int = 0;
            pub static R_OK : c_int = 4;
            pub static W_OK : c_int = 2;
            pub static X_OK : c_int = 1;
            pub static STDIN_FILENO : c_int = 0;
            pub static STDOUT_FILENO : c_int = 1;
            pub static STDERR_FILENO : c_int = 2;
            pub static F_LOCK : c_int = 1;
            pub static F_TEST : c_int = 3;
            pub static F_TLOCK : c_int = 2;
            pub static F_ULOCK : c_int = 0;
            pub static SIGHUP : c_int = 1;
            pub static SIGINT : c_int = 2;
            pub static SIGQUIT : c_int = 3;
            pub static SIGILL : c_int = 4;
            pub static SIGABRT : c_int = 6;
            pub static SIGFPE : c_int = 8;
            pub static SIGKILL : c_int = 9;
            pub static SIGSEGV : c_int = 11;
            pub static SIGPIPE : c_int = 13;
            pub static SIGALRM : c_int = 14;
            pub static SIGTERM : c_int = 15;

            pub static PROT_NONE : c_int = 0;
            pub static PROT_READ : c_int = 1;
            pub static PROT_WRITE : c_int = 2;
            pub static PROT_EXEC : c_int = 4;

            pub static MAP_FILE : c_int = 0x0000;
            pub static MAP_SHARED : c_int = 0x0001;
            pub static MAP_PRIVATE : c_int = 0x0002;
            pub static MAP_FIXED : c_int = 0x0010;
            pub static MAP_ANON : c_int = 0x1000;

            pub static MAP_FAILED : *c_void = -1 as *c_void;

            pub static MCL_CURRENT : c_int = 0x0001;
            pub static MCL_FUTURE : c_int = 0x0002;

            pub static MS_ASYNC : c_int = 0x0001;
            pub static MS_INVALIDATE : c_int = 0x0002;
            pub static MS_SYNC : c_int = 0x0010;

            pub static MS_KILLPAGES : c_int = 0x0004;
            pub static MS_DEACTIVATE : c_int = 0x0008;

            pub static EPERM : c_int = 1;
            pub static ENOENT : c_int = 2;
            pub static ESRCH : c_int = 3;
            pub static EINTR : c_int = 4;
            pub static EIO : c_int = 5;
            pub static ENXIO : c_int = 6;
            pub static E2BIG : c_int = 7;
            pub static ENOEXEC : c_int = 8;
            pub static EBADF : c_int = 9;
            pub static ECHILD : c_int = 10;
            pub static EDEADLK : c_int = 11;
            pub static ENOMEM : c_int = 12;
            pub static EACCES : c_int = 13;
            pub static EFAULT : c_int = 14;
            pub static ENOTBLK : c_int = 15;
            pub static EBUSY : c_int = 16;
            pub static EEXIST : c_int = 17;
            pub static EXDEV : c_int = 18;
            pub static ENODEV : c_int = 19;
            pub static ENOTDIR : c_int = 20;
            pub static EISDIR : c_int = 21;
            pub static EINVAL : c_int = 22;
            pub static ENFILE : c_int = 23;
            pub static EMFILE : c_int = 24;
            pub static ENOTTY : c_int = 25;
            pub static ETXTBSY : c_int = 26;
            pub static EFBIG : c_int = 27;
            pub static ENOSPC : c_int = 28;
            pub static ESPIPE : c_int = 29;
            pub static EROFS : c_int = 30;
            pub static EMLINK : c_int = 31;
            pub static EPIPE : c_int = 32;
            pub static EDOM : c_int = 33;
            pub static ERANGE : c_int = 34;
            pub static EAGAIN : c_int = 35;
            pub static EWOULDBLOCK : c_int = EAGAIN;
            pub static EINPROGRESS : c_int = 36;
            pub static EALREADY : c_int = 37;
            pub static ENOTSOCK : c_int = 38;
            pub static EDESTADDRREQ : c_int = 39;
            pub static EMSGSIZE : c_int = 40;
            pub static EPROTOTYPE : c_int = 41;
            pub static ENOPROTOOPT : c_int = 42;
            pub static EPROTONOSUPPORT : c_int = 43;
            pub static ESOCKTNOSUPPORT : c_int = 44;
            pub static ENOTSUP : c_int = 45;
            pub static EPFNOSUPPORT : c_int = 46;
            pub static EAFNOSUPPORT : c_int = 47;
            pub static EADDRINUSE : c_int = 48;
            pub static EADDRNOTAVAIL : c_int = 49;
            pub static ENETDOWN : c_int = 50;
            pub static ENETUNREACH : c_int = 51;
            pub static ENETRESET : c_int = 52;
            pub static ECONNABORTED : c_int = 53;
            pub static ECONNRESET : c_int = 54;
            pub static ENOBUFS : c_int = 55;
            pub static EISCONN : c_int = 56;
            pub static ENOTCONN : c_int = 57;
            pub static ESHUTDOWN : c_int = 58;
            pub static ETOOMANYREFS : c_int = 59;
            pub static ETIMEDOUT : c_int = 60;
            pub static ECONNREFUSED : c_int = 61;
            pub static ELOOP : c_int = 62;
            pub static ENAMETOOLONG : c_int = 63;
            pub static EHOSTDOWN : c_int = 64;
            pub static EHOSTUNREACH : c_int = 65;
            pub static ENOTEMPTY : c_int = 66;
            pub static EPROCLIM : c_int = 67;
            pub static EUSERS : c_int = 68;
            pub static EDQUOT : c_int = 69;
            pub static ESTALE : c_int = 70;
            pub static EREMOTE : c_int = 71;
            pub static EBADRPC : c_int = 72;
            pub static ERPCMISMATCH : c_int = 73;
            pub static EPROGUNAVAIL : c_int = 74;
            pub static EPROGMISMATCH : c_int = 75;
            pub static EPROCUNAVAIL : c_int = 76;
            pub static ENOLCK : c_int = 77;
            pub static ENOSYS : c_int = 78;
            pub static EFTYPE : c_int = 79;
            pub static EAUTH : c_int = 80;
            pub static ENEEDAUTH : c_int = 81;
            pub static EPWROFF : c_int = 82;
            pub static EDEVERR : c_int = 83;
            pub static EOVERFLOW : c_int = 84;
            pub static EBADEXEC : c_int = 85;
            pub static EBADARCH : c_int = 86;
            pub static ESHLIBVERS : c_int = 87;
            pub static EBADMACHO : c_int = 88;
            pub static ECANCELED : c_int = 89;
            pub static EIDRM : c_int = 90;
            pub static ENOMSG : c_int = 91;
            pub static EILSEQ : c_int = 92;
            pub static ENOATTR : c_int = 93;
            pub static EBADMSG : c_int = 94;
            pub static EMULTIHOP : c_int = 95;
            pub static ENODATA : c_int = 96;
            pub static ENOLINK : c_int = 97;
            pub static ENOSR : c_int = 98;
            pub static ENOSTR : c_int = 99;
            pub static EPROTO : c_int = 100;
            pub static ETIME : c_int = 101;
            pub static EOPNOTSUPP : c_int = 102;
            pub static ENOPOLICY : c_int = 103;
            pub static ENOTRECOVERABLE : c_int = 104;
            pub static EOWNERDEAD : c_int = 105;
            pub static EQFULL : c_int = 106;
            pub static ELAST : c_int = 106;
        }
        pub mod posix01 {
            use libc::types::os::arch::c95::c_int;

            pub static SIGTRAP : c_int = 5;

            pub static GLOB_APPEND   : c_int = 0x0001;
            pub static GLOB_DOOFFS   : c_int = 0x0002;
            pub static GLOB_ERR      : c_int = 0x0004;
            pub static GLOB_MARK     : c_int = 0x0008;
            pub static GLOB_NOCHECK  : c_int = 0x0010;
            pub static GLOB_NOSORT   : c_int = 0x0020;
            pub static GLOB_NOESCAPE : c_int = 0x2000;

            pub static GLOB_NOSPACE  : c_int = -1;
            pub static GLOB_ABORTED  : c_int = -2;
            pub static GLOB_NOMATCH  : c_int = -3;

            pub static POSIX_MADV_NORMAL : c_int = 0;
            pub static POSIX_MADV_RANDOM : c_int = 1;
            pub static POSIX_MADV_SEQUENTIAL : c_int = 2;
            pub static POSIX_MADV_WILLNEED : c_int = 3;
            pub static POSIX_MADV_DONTNEED : c_int = 4;

            pub static _SC_IOV_MAX : c_int = 56;
            pub static _SC_GETGR_R_SIZE_MAX : c_int = 70;
            pub static _SC_GETPW_R_SIZE_MAX : c_int = 71;
            pub static _SC_LOGIN_NAME_MAX : c_int = 73;
            pub static _SC_MQ_PRIO_MAX : c_int = 75;
            pub static _SC_THREAD_ATTR_STACKADDR : c_int = 82;
            pub static _SC_THREAD_ATTR_STACKSIZE : c_int = 83;
            pub static _SC_THREAD_DESTRUCTOR_ITERATIONS : c_int = 85;
            pub static _SC_THREAD_KEYS_MAX : c_int = 86;
            pub static _SC_THREAD_PRIO_INHERIT : c_int = 87;
            pub static _SC_THREAD_PRIO_PROTECT : c_int = 88;
            pub static _SC_THREAD_PRIORITY_SCHEDULING : c_int = 89;
            pub static _SC_THREAD_PROCESS_SHARED : c_int = 90;
            pub static _SC_THREAD_SAFE_FUNCTIONS : c_int = 91;
            pub static _SC_THREAD_STACK_MIN : c_int = 93;
            pub static _SC_THREAD_THREADS_MAX : c_int = 94;
            pub static _SC_THREADS : c_int = 96;
            pub static _SC_TTY_NAME_MAX : c_int = 101;
            pub static _SC_ATEXIT_MAX : c_int = 107;
            pub static _SC_XOPEN_CRYPT : c_int = 108;
            pub static _SC_XOPEN_ENH_I18N : c_int = 109;
            pub static _SC_XOPEN_LEGACY : c_int = 110;
            pub static _SC_XOPEN_REALTIME : c_int = 111;
            pub static _SC_XOPEN_REALTIME_THREADS : c_int = 112;
            pub static _SC_XOPEN_SHM : c_int = 113;
            pub static _SC_XOPEN_UNIX : c_int = 115;
            pub static _SC_XOPEN_VERSION : c_int = 116;
            pub static _SC_XOPEN_XCU_VERSION : c_int = 121;
        }
        pub mod posix08 {
        }
        pub mod bsd44 {
            use libc::types::os::arch::c95::c_int;

            pub static MADV_NORMAL : c_int = 0;
            pub static MADV_RANDOM : c_int = 1;
            pub static MADV_SEQUENTIAL : c_int = 2;
            pub static MADV_WILLNEED : c_int = 3;
            pub static MADV_DONTNEED : c_int = 4;
            pub static MADV_FREE : c_int = 5;
            pub static MADV_ZERO_WIRED_PAGES : c_int = 6;
            pub static MADV_FREE_REUSABLE : c_int = 7;
            pub static MADV_FREE_REUSE : c_int = 8;
            pub static MADV_CAN_REUSE : c_int = 9;

            pub static MINCORE_INCORE : c_int =  0x1;
            pub static MINCORE_REFERENCED : c_int = 0x2;
            pub static MINCORE_MODIFIED : c_int = 0x4;
            pub static MINCORE_REFERENCED_OTHER : c_int = 0x8;
            pub static MINCORE_MODIFIED_OTHER : c_int = 0x10;
        }
        pub mod extra {
            use libc::types::os::arch::c95::c_int;

            pub static O_DSYNC : c_int = 4194304;
            pub static O_SYNC : c_int = 128;
            pub static F_FULLFSYNC : c_int = 51;

            pub static MAP_COPY : c_int = 0x0002;
            pub static MAP_RENAME : c_int = 0x0020;
            pub static MAP_NORESERVE : c_int = 0x0040;
            pub static MAP_NOEXTEND : c_int = 0x0100;
            pub static MAP_HASSEMAPHORE : c_int = 0x0200;
            pub static MAP_NOCACHE : c_int = 0x0400;
            pub static MAP_JIT : c_int = 0x0800;
        }
        pub mod sysconf {
            use libc::types::os::arch::c95::c_int;

            pub static _SC_ARG_MAX : c_int = 1;
            pub static _SC_CHILD_MAX : c_int = 2;
            pub static _SC_CLK_TCK : c_int = 3;
            pub static _SC_NGROUPS_MAX : c_int = 4;
            pub static _SC_OPEN_MAX : c_int = 5;
            pub static _SC_JOB_CONTROL : c_int = 6;
            pub static _SC_SAVED_IDS : c_int = 7;
            pub static _SC_VERSION : c_int = 8;
            pub static _SC_BC_BASE_MAX : c_int = 9;
            pub static _SC_BC_DIM_MAX : c_int = 10;
            pub static _SC_BC_SCALE_MAX : c_int = 11;
            pub static _SC_BC_STRING_MAX : c_int = 12;
            pub static _SC_COLL_WEIGHTS_MAX : c_int = 13;
            pub static _SC_EXPR_NEST_MAX : c_int = 14;
            pub static _SC_LINE_MAX : c_int = 15;
            pub static _SC_RE_DUP_MAX : c_int = 16;
            pub static _SC_2_VERSION : c_int = 17;
            pub static _SC_2_C_BIND : c_int = 18;
            pub static _SC_2_C_DEV : c_int = 19;
            pub static _SC_2_CHAR_TERM : c_int = 20;
            pub static _SC_2_FORT_DEV : c_int = 21;
            pub static _SC_2_FORT_RUN : c_int = 22;
            pub static _SC_2_LOCALEDEF : c_int = 23;
            pub static _SC_2_SW_DEV : c_int = 24;
            pub static _SC_2_UPE : c_int = 25;
            pub static _SC_STREAM_MAX : c_int = 26;
            pub static _SC_TZNAME_MAX : c_int = 27;
            pub static _SC_ASYNCHRONOUS_IO : c_int = 28;
            pub static _SC_PAGESIZE : c_int = 29;
            pub static _SC_MEMLOCK : c_int = 30;
            pub static _SC_MEMLOCK_RANGE : c_int = 31;
            pub static _SC_MEMORY_PROTECTION : c_int = 32;
            pub static _SC_MESSAGE_PASSING : c_int = 33;
            pub static _SC_PRIORITIZED_IO : c_int = 34;
            pub static _SC_PRIORITY_SCHEDULING : c_int = 35;
            pub static _SC_REALTIME_SIGNALS : c_int = 36;
            pub static _SC_SEMAPHORES : c_int = 37;
            pub static _SC_FSYNC : c_int = 38;
            pub static _SC_SHARED_MEMORY_OBJECTS : c_int = 39;
            pub static _SC_SYNCHRONIZED_IO : c_int = 40;
            pub static _SC_TIMERS : c_int = 41;
            pub static _SC_AIO_LISTIO_MAX : c_int = 42;
            pub static _SC_AIO_MAX : c_int = 43;
            pub static _SC_AIO_PRIO_DELTA_MAX : c_int = 44;
            pub static _SC_DELAYTIMER_MAX : c_int = 45;
            pub static _SC_MQ_OPEN_MAX : c_int = 46;
            pub static _SC_MAPPED_FILES : c_int = 47;
            pub static _SC_RTSIG_MAX : c_int = 48;
            pub static _SC_SEM_NSEMS_MAX : c_int = 49;
            pub static _SC_SEM_VALUE_MAX : c_int = 50;
            pub static _SC_SIGQUEUE_MAX : c_int = 51;
            pub static _SC_TIMER_MAX : c_int = 52;
            pub static _SC_XBS5_ILP32_OFF32 : c_int = 122;
            pub static _SC_XBS5_ILP32_OFFBIG : c_int = 123;
            pub static _SC_XBS5_LP64_OFF64 : c_int = 124;
            pub static _SC_XBS5_LPBIG_OFFBIG : c_int = 125;
        }
    }
}


pub mod funcs {
    // Thankfull most of c95 is universally available and does not vary by OS
    // or anything. The same is not true of POSIX.

    pub mod c95 {
        #[nolink]
        pub mod ctype {
            use libc::types::os::arch::c95::{c_char, c_int};

            extern {
                pub fn isalnum(c: c_int) -> c_int;
                pub fn isalpha(c: c_int) -> c_int;
                pub fn iscntrl(c: c_int) -> c_int;
                pub fn isdigit(c: c_int) -> c_int;
                pub fn isgraph(c: c_int) -> c_int;
                pub fn islower(c: c_int) -> c_int;
                pub fn isprint(c: c_int) -> c_int;
                pub fn ispunct(c: c_int) -> c_int;
                pub fn isspace(c: c_int) -> c_int;
                pub fn isupper(c: c_int) -> c_int;
                pub fn isxdigit(c: c_int) -> c_int;
                pub fn tolower(c: c_char) -> c_char;
                pub fn toupper(c: c_char) -> c_char;
            }
        }

        #[nolink]
        pub mod stdio {
            use libc::types::common::c95::{FILE, c_void, fpos_t};
            use libc::types::os::arch::c95::{c_char, c_int, c_long, size_t};

            extern {
                pub fn fopen(filename: *c_char, mode: *c_char) -> *FILE;
                pub fn freopen(filename: *c_char, mode: *c_char, file: *FILE)
                               -> *FILE;
                pub fn fflush(file: *FILE) -> c_int;
                pub fn fclose(file: *FILE) -> c_int;
                pub fn remove(filename: *c_char) -> c_int;
                pub fn rename(oldname: *c_char, newname: *c_char) -> c_int;
                pub fn tmpfile() -> *FILE;
                pub fn setvbuf(stream: *FILE,
                               buffer: *c_char,
                               mode: c_int,
                               size: size_t)
                               -> c_int;
                pub fn setbuf(stream: *FILE, buf: *c_char);
                // Omitted: printf and scanf variants.
                pub fn fgetc(stream: *FILE) -> c_int;
                pub fn fgets(buf: *mut c_char, n: c_int, stream: *FILE)
                             -> *c_char;
                pub fn fputc(c: c_int, stream: *FILE) -> c_int;
                pub fn fputs(s: *c_char, stream: *FILE) -> *c_char;
                // Omitted: getc, getchar (might be macros).

                // Omitted: gets, so ridiculously unsafe that it should not
                // survive.

                // Omitted: putc, putchar (might be macros).
                pub fn puts(s: *c_char) -> c_int;
                pub fn ungetc(c: c_int, stream: *FILE) -> c_int;
                pub fn fread(ptr: *mut c_void,
                             size: size_t,
                             nobj: size_t,
                             stream: *FILE)
                             -> size_t;
                pub fn fwrite(ptr: *c_void,
                              size: size_t,
                              nobj: size_t,
                              stream: *FILE)
                              -> size_t;
                pub fn fseek(stream: *FILE, offset: c_long, whence: c_int)
                             -> c_int;
                pub fn ftell(stream: *FILE) -> c_long;
                pub fn rewind(stream: *FILE);
                pub fn fgetpos(stream: *FILE, ptr: *fpos_t) -> c_int;
                pub fn fsetpos(stream: *FILE, ptr: *fpos_t) -> c_int;
                pub fn feof(stream: *FILE) -> c_int;
                pub fn ferror(stream: *FILE) -> c_int;
                pub fn perror(s: *c_char);
            }
        }

        #[nolink]
        pub mod stdlib {
            use libc::types::common::c95::c_void;
            use libc::types::os::arch::c95::{c_char, c_double, c_int};
            use libc::types::os::arch::c95::{c_long, c_uint, c_ulong};
            use libc::types::os::arch::c95::{size_t};

            extern {
                pub fn abs(i: c_int) -> c_int;
                pub fn labs(i: c_long) -> c_long;
                // Omitted: div, ldiv (return pub type incomplete).
                pub fn atof(s: *c_char) -> c_double;
                pub fn atoi(s: *c_char) -> c_int;
                pub fn strtod(s: *c_char, endp: **c_char) -> c_double;
                pub fn strtol(s: *c_char, endp: **c_char, base: c_int)
                              -> c_long;
                pub fn strtoul(s: *c_char, endp: **c_char, base: c_int)
                               -> c_ulong;
                pub fn calloc(nobj: size_t, size: size_t) -> *c_void;
                pub fn malloc(size: size_t) -> *c_void;
                pub fn realloc(p: *mut c_void, size: size_t) -> *mut c_void;
                pub fn free(p: *c_void);
                pub fn abort() -> !;
                pub fn exit(status: c_int) -> !;
                // Omitted: atexit.
                pub fn system(s: *c_char) -> c_int;
                pub fn getenv(s: *c_char) -> *c_char;
                // Omitted: bsearch, qsort
                pub fn rand() -> c_int;
                pub fn srand(seed: c_uint);
            }
        }

        #[nolink]
        pub mod string {
            use libc::types::common::c95::c_void;
            use libc::types::os::arch::c95::{c_char, c_int, size_t};
            use libc::types::os::arch::c95::{wchar_t};

            extern {
                pub fn strcpy(dst: *c_char, src: *c_char) -> *c_char;
                pub fn strncpy(dst: *c_char, src: *c_char, n: size_t)
                               -> *c_char;
                pub fn strcat(s: *c_char, ct: *c_char) -> *c_char;
                pub fn strncat(s: *c_char, ct: *c_char, n: size_t) -> *c_char;
                pub fn strcmp(cs: *c_char, ct: *c_char) -> c_int;
                pub fn strncmp(cs: *c_char, ct: *c_char, n: size_t) -> c_int;
                pub fn strcoll(cs: *c_char, ct: *c_char) -> c_int;
                pub fn strchr(cs: *c_char, c: c_int) -> *c_char;
                pub fn strrchr(cs: *c_char, c: c_int) -> *c_char;
                pub fn strspn(cs: *c_char, ct: *c_char) -> size_t;
                pub fn strcspn(cs: *c_char, ct: *c_char) -> size_t;
                pub fn strpbrk(cs: *c_char, ct: *c_char) -> *c_char;
                pub fn strstr(cs: *c_char, ct: *c_char) -> *c_char;
                pub fn strlen(cs: *c_char) -> size_t;
                pub fn strerror(n: c_int) -> *c_char;
                pub fn strtok(s: *c_char, t: *c_char) -> *c_char;
                pub fn strxfrm(s: *c_char, ct: *c_char, n: size_t) -> size_t;
                pub fn wcslen(buf: *wchar_t) -> size_t;

                // Omitted: memcpy, memmove, memset (provided by LLVM)

                // These are fine to execute on the Rust stack. They must be,
                // in fact, because LLVM generates calls to them!
                #[rust_stack]
                #[inline]
                pub fn memcmp(cx: *c_void, ct: *c_void, n: size_t) -> c_int;
                #[rust_stack]
                #[inline]
                pub fn memchr(cx: *c_void, c: c_int, n: size_t) -> *c_void;
            }
        }
    }

    // Microsoft helpfully underscore-qualifies all of its POSIX-like symbols
    // to make sure you don't use them accidentally. It also randomly deviates
    // from the exact signatures you might otherwise expect, and omits much,
    // so be careful when trying to write portable code; it won't always work
    // with the same POSIX functions and types as other platforms.

    #[cfg(target_os = "win32")]
    pub mod posix88 {
        #[nolink]
        pub mod stat_ {
            use libc::types::os::common::posix01::stat;
            use libc::types::os::arch::c95::{c_int, c_char};

            extern {
                #[link_name = "_chmod"]
                pub fn chmod(path: *c_char, mode: c_int) -> c_int;
                #[link_name = "_mkdir"]
                pub fn mkdir(path: *c_char) -> c_int;
                #[link_name = "_fstat64"]
                pub fn fstat(fildes: c_int, buf: *mut stat) -> c_int;
                #[link_name = "_stat64"]
                pub fn stat(path: *c_char, buf: *mut stat) -> c_int;
            }
        }

        #[nolink]
        pub mod stdio {
            use libc::types::common::c95::FILE;
            use libc::types::os::arch::c95::{c_int, c_char};

            extern {
                #[link_name = "_popen"]
                pub fn popen(command: *c_char, mode: *c_char) -> *FILE;
                #[link_name = "_pclose"]
                pub fn pclose(stream: *FILE) -> c_int;
                #[link_name = "_fdopen"]
                pub fn fdopen(fd: c_int, mode: *c_char) -> *FILE;
                #[link_name = "_fileno"]
                pub fn fileno(stream: *FILE) -> c_int;
            }
        }

        #[nolink]
        pub mod fcntl {
            use libc::types::os::arch::c95::{c_int, c_char};
            extern {
                #[link_name = "_open"]
                pub fn open(path: *c_char, oflag: c_int, mode: c_int)
                            -> c_int;
                #[link_name = "_creat"]
                pub fn creat(path: *c_char, mode: c_int) -> c_int;
            }
        }

        #[nolink]
        pub mod dirent {
            // Not supplied at all.
        }

        #[nolink]
        pub mod unistd {
            use libc::types::common::c95::c_void;
            use libc::types::os::arch::c95::{c_int, c_uint, c_char,
                                             c_long, size_t};
            use libc::types::os::arch::c99::intptr_t;

            extern {
                #[link_name = "_access"]
                pub fn access(path: *c_char, amode: c_int) -> c_int;
                #[link_name = "_chdir"]
                pub fn chdir(dir: *c_char) -> c_int;
                #[link_name = "_close"]
                pub fn close(fd: c_int) -> c_int;
                #[link_name = "_dup"]
                pub fn dup(fd: c_int) -> c_int;
                #[link_name = "_dup2"]
                pub fn dup2(src: c_int, dst: c_int) -> c_int;
                #[link_name = "_execv"]
                pub fn execv(prog: *c_char, argv: **c_char) -> intptr_t;
                #[link_name = "_execve"]
                pub fn execve(prog: *c_char, argv: **c_char, envp: **c_char)
                              -> c_int;
                #[link_name = "_execvp"]
                pub fn execvp(c: *c_char, argv: **c_char) -> c_int;
                #[link_name = "_execvpe"]
                pub fn execvpe(c: *c_char, argv: **c_char, envp: **c_char)
                               -> c_int;
                #[link_name = "_getcwd"]
                pub fn getcwd(buf: *mut c_char, size: size_t) -> *c_char;
                #[link_name = "_getpid"]
                pub fn getpid() -> c_int;
                #[link_name = "_isatty"]
                pub fn isatty(fd: c_int) -> c_int;
                #[link_name = "_lseek"]
                pub fn lseek(fd: c_int, offset: c_long, origin: c_int)
                             -> c_long;
                #[link_name = "_pipe"]
                pub fn pipe(fds: *mut c_int, psize: c_uint, textmode: c_int)
                            -> c_int;
                #[link_name = "_read"]
                pub fn read(fd: c_int, buf: *mut c_void, count: c_uint)
                            -> c_int;
                #[link_name = "_rmdir"]
                pub fn rmdir(path: *c_char) -> c_int;
                #[link_name = "_unlink"]
                pub fn unlink(c: *c_char) -> c_int;
                #[link_name = "_write"]
                pub fn write(fd: c_int, buf: *c_void, count: c_uint) -> c_int;
            }
        }

        pub mod mman {
        }
    }


    #[cfg(target_os = "linux")]
    #[cfg(target_os = "android")]
    #[cfg(target_os = "macos")]
    #[cfg(target_os = "freebsd")]
    pub mod posix88 {
        pub mod stat_ {
            use libc::types::os::arch::c95::{c_char, c_int};
            use libc::types::os::arch::posix01::stat;
            use libc::types::os::arch::posix88::mode_t;

            #[nolink]
            extern {
                pub fn chmod(path: *c_char, mode: mode_t) -> c_int;
                pub fn fchmod(fd: c_int, mode: mode_t) -> c_int;

                #[cfg(target_os = "linux")]
                #[cfg(target_os = "freebsd")]
                #[cfg(target_os = "android")]
                pub fn fstat(fildes: c_int, buf: *mut stat) -> c_int;

                #[cfg(target_os = "macos")]
                #[link_name = "fstat64"]
                pub fn fstat(fildes: c_int, buf: *mut stat) -> c_int;

                pub fn mkdir(path: *c_char, mode: mode_t) -> c_int;
                pub fn mkfifo(path: *c_char, mode: mode_t) -> c_int;

                #[cfg(target_os = "linux")]
                #[cfg(target_os = "freebsd")]
                #[cfg(target_os = "android")]
                pub fn stat(path: *c_char, buf: *mut stat) -> c_int;

                #[cfg(target_os = "macos")]
                #[link_name = "stat64"]
                pub fn stat(path: *c_char, buf: *mut stat) -> c_int;
            }
        }

        #[nolink]
        pub mod stdio {
            use libc::types::common::c95::FILE;
            use libc::types::os::arch::c95::{c_char, c_int};

            extern {
                pub fn popen(command: *c_char, mode: *c_char) -> *FILE;
                pub fn pclose(stream: *FILE) -> c_int;
                pub fn fdopen(fd: c_int, mode: *c_char) -> *FILE;
                pub fn fileno(stream: *FILE) -> c_int;
            }
        }

        #[nolink]
        pub mod fcntl {
            use libc::types::os::arch::c95::{c_char, c_int};
            use libc::types::os::arch::posix88::mode_t;

            extern {
                pub fn open(path: *c_char, oflag: c_int, mode: c_int)
                            -> c_int;
                pub fn creat(path: *c_char, mode: mode_t) -> c_int;
                pub fn fcntl(fd: c_int, cmd: c_int) -> c_int;
            }
        }

        #[nolink]
        pub mod dirent {
            use libc::types::common::posix88::{DIR, dirent_t};
            use libc::types::os::arch::c95::{c_char, c_int, c_long};

            // NB: On OS X opendir and readdir have two versions,
            // one for 32-bit kernelspace and one for 64.
            // We should be linking to the 64-bit ones, called
            // opendir$INODE64, etc. but for some reason rustc
            // doesn't link it correctly on i686, so we're going
            // through a C function that mysteriously does work.
            pub unsafe fn opendir(dirname: *c_char) -> *DIR {
                #[fixed_stack_segment]; #[inline(never)];
                rust_opendir(dirname)
            }
            pub unsafe fn readdir(dirp: *DIR) -> *dirent_t {
                #[fixed_stack_segment]; #[inline(never)];
                rust_readdir(dirp)
            }

            extern {
                fn rust_opendir(dirname: *c_char) -> *DIR;
                fn rust_readdir(dirp: *DIR) -> *dirent_t;
            }

            extern {
                pub fn closedir(dirp: *DIR) -> c_int;
                pub fn rewinddir(dirp: *DIR);
                pub fn seekdir(dirp: *DIR, loc: c_long);
                pub fn telldir(dirp: *DIR) -> c_long;
            }
        }

        #[nolink]
        pub mod unistd {
            use libc::types::common::c95::c_void;
            use libc::types::os::arch::c95::{c_char, c_int, c_long, c_uint};
            use libc::types::os::arch::c95::{size_t};
            use libc::types::os::arch::posix88::{gid_t, off_t, pid_t};
            use libc::types::os::arch::posix88::{ssize_t, uid_t};

            extern {
                pub fn access(path: *c_char, amode: c_int) -> c_int;
                pub fn alarm(seconds: c_uint) -> c_uint;
                pub fn chdir(dir: *c_char) -> c_int;
                pub fn chown(path: *c_char, uid: uid_t, gid: gid_t) -> c_int;
                pub fn close(fd: c_int) -> c_int;
                pub fn dup(fd: c_int) -> c_int;
                pub fn dup2(src: c_int, dst: c_int) -> c_int;
                pub fn execv(prog: *c_char, argv: **c_char) -> c_int;
                pub fn execve(prog: *c_char, argv: **c_char, envp: **c_char)
                              -> c_int;
                pub fn execvp(c: *c_char, argv: **c_char) -> c_int;
                pub fn fork() -> pid_t;
                pub fn fpathconf(filedes: c_int, name: c_int) -> c_long;
                pub fn getcwd(buf: *mut c_char, size: size_t) -> *c_char;
                pub fn getegid() -> gid_t;
                pub fn geteuid() -> uid_t;
                pub fn getgid() -> gid_t ;
                pub fn getgroups(ngroups_max: c_int, groups: *mut gid_t)
                                 -> c_int;
                pub fn getlogin() -> *c_char;
                pub fn getopt(argc: c_int, argv: **c_char, optstr: *c_char)
                              -> c_int;
                pub fn getpgrp() -> pid_t;
                pub fn getpid() -> pid_t;
                pub fn getppid() -> pid_t;
                pub fn getuid() -> uid_t;
                pub fn isatty(fd: c_int) -> c_int;
                pub fn link(src: *c_char, dst: *c_char) -> c_int;
                pub fn lseek(fd: c_int, offset: off_t, whence: c_int)
                             -> off_t;
                pub fn pathconf(path: *c_char, name: c_int) -> c_long;
                pub fn pause() -> c_int;
                pub fn pipe(fds: *mut c_int) -> c_int;
                pub fn read(fd: c_int, buf: *mut c_void, count: size_t)
                            -> ssize_t;
                pub fn rmdir(path: *c_char) -> c_int;
                pub fn setgid(gid: gid_t) -> c_int;
                pub fn setpgid(pid: pid_t, pgid: pid_t) -> c_int;
                pub fn setsid() -> pid_t;
                pub fn setuid(uid: uid_t) -> c_int;
                pub fn sleep(secs: c_uint) -> c_uint;
                pub fn sysconf(name: c_int) -> c_long;
                pub fn tcgetpgrp(fd: c_int) -> pid_t;
                pub fn ttyname(fd: c_int) -> *c_char;
                pub fn unlink(c: *c_char) -> c_int;
                pub fn write(fd: c_int, buf: *c_void, count: size_t)
                             -> ssize_t;
            }
        }

        #[nolink]
        pub mod signal {
            use libc::types::os::arch::c95::{c_int};
            use libc::types::os::arch::posix88::{pid_t};

            extern {
                pub fn kill(pid: pid_t, sig: c_int) -> c_int;
            }
        }

        #[nolink]
        pub mod mman {
            use libc::types::common::c95::{c_void};
            use libc::types::os::arch::c95::{size_t, c_int, c_char};
            use libc::types::os::arch::posix88::{mode_t, off_t};

            extern {
                pub fn mlock(addr: *c_void, len: size_t) -> c_int;
                pub fn munlock(addr: *c_void, len: size_t) -> c_int;
                pub fn mlockall(flags: c_int) -> c_int;
                pub fn munlockall() -> c_int;

                pub fn mmap(addr: *c_void,
                            len: size_t,
                            prot: c_int,
                            flags: c_int,
                            fd: c_int,
                            offset: off_t)
                            -> *mut c_void;
                pub fn munmap(addr: *c_void, len: size_t) -> c_int;

                pub fn mprotect(addr: *c_void, len: size_t, prot: c_int)
                                -> c_int;

                pub fn msync(addr: *c_void, len: size_t, flags: c_int)
                             -> c_int;
                pub fn shm_open(name: *c_char, oflag: c_int, mode: mode_t)
                                -> c_int;
                pub fn shm_unlink(name: *c_char) -> c_int;
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[cfg(target_os = "android")]
    #[cfg(target_os = "macos")]
    #[cfg(target_os = "freebsd")]
    pub mod posix01 {
        #[nolink]
        pub mod stat_ {
            use libc::types::os::arch::c95::{c_char, c_int};
            use libc::types::os::arch::posix01::stat;

            extern {
                #[cfg(target_os = "linux")]
                #[cfg(target_os = "freebsd")]
                #[cfg(target_os = "android")]
                pub fn lstat(path: *c_char, buf: *mut stat) -> c_int;

                #[cfg(target_os = "macos")]
                #[link_name = "lstat64"]
                pub fn lstat(path: *c_char, buf: *mut stat) -> c_int;
            }
        }

        #[nolink]
        pub mod unistd {
            use libc::types::os::arch::c95::{c_char, c_int, size_t};
            use libc::types::os::arch::posix88::{ssize_t};

            extern {
                pub fn readlink(path: *c_char,
                                buf: *mut c_char,
                                bufsz: size_t)
                                -> ssize_t;

                pub fn fsync(fd: c_int) -> c_int;

                #[cfg(target_os = "linux")]
                #[cfg(target_os = "android")]
                pub fn fdatasync(fd: c_int) -> c_int;

                pub fn setenv(name: *c_char, val: *c_char, overwrite: c_int)
                              -> c_int;
                pub fn unsetenv(name: *c_char) -> c_int;
                pub fn putenv(string: *c_char) -> c_int;

                pub fn symlink(path1: *c_char, path2: *c_char) -> c_int;
            }
        }

        #[nolink]
        pub mod wait {
            use libc::types::os::arch::c95::{c_int};
            use libc::types::os::arch::posix88::{pid_t};

            extern {
                pub fn waitpid(pid: pid_t, status: *mut c_int, options: c_int)
                               -> pid_t;
            }
        }

        #[nolink]
        pub mod glob {
            use libc::types::os::arch::c95::{c_char, c_int};
            use libc::types::os::common::posix01::{glob_t};
            use option::Option;

            extern {
                pub fn glob(pattern: *c_char,
                            flags: c_int,
                            errfunc: Option<extern "C" fn(epath: *c_char, errno: int) -> int>,
                            pglob: *mut glob_t);
                pub fn globfree(pglob: *mut glob_t);
            }
        }

        #[nolink]
        pub mod mman {
            use libc::types::common::c95::{c_void};
            use libc::types::os::arch::c95::{c_int, size_t};

            extern {
                pub fn posix_madvise(addr: *c_void,
                                     len: size_t,
                                     advice: c_int)
                                     -> c_int;
            }
        }
    }

    #[cfg(target_os = "win32")]
    pub mod posix01 {
        pub mod stat_ {
        }

        pub mod unistd {
        }

        pub mod glob {
        }

        pub mod mman {
        }
    }


    #[cfg(target_os = "win32")]
    #[cfg(target_os = "linux")]
    #[cfg(target_os = "android")]
    #[cfg(target_os = "macos")]
    #[cfg(target_os = "freebsd")]
    pub mod posix08 {
        pub mod unistd {
        }
    }


    #[cfg(target_os = "macos")]
    #[cfg(target_os = "freebsd")]
    pub mod bsd44 {
        use libc::types::common::c95::{c_void};
        use libc::types::os::arch::c95::{c_char, c_uchar, c_int, c_uint,
                                         size_t};

        extern {
            pub fn sysctl(name: *c_int,
                          namelen: c_uint,
                          oldp: *mut c_void,
                          oldlenp: *mut size_t,
                          newp: *c_void,
                          newlen: size_t)
                          -> c_int;
            pub fn sysctlbyname(name: *c_char,
                                oldp: *mut c_void,
                                oldlenp: *mut size_t,
                                newp: *c_void,
                                newlen: size_t)
                                -> c_int;
            pub fn sysctlnametomib(name: *c_char,
                                   mibp: *mut c_int,
                                   sizep: *mut size_t)
                                   -> c_int;
            pub fn getdtablesize() -> c_int;
            pub fn madvise(addr: *c_void, len: size_t, advice: c_int)
                           -> c_int;
            pub fn mincore(addr: *c_void, len: size_t, vec: *c_uchar)
                           -> c_int;
        }
    }


    #[cfg(target_os = "linux")]
    #[cfg(target_os = "android")]
    pub mod bsd44 {
        use libc::types::common::c95::{c_void};
        use libc::types::os::arch::c95::{c_uchar, c_int, size_t};

        extern {
            pub fn getdtablesize() -> c_int;
            pub fn madvise(addr: *c_void, len: size_t, advice: c_int)
                           -> c_int;
            pub fn mincore(addr: *c_void, len: size_t, vec: *c_uchar)
                           -> c_int;
        }
    }


    #[cfg(target_os = "win32")]
    pub mod bsd44 {
    }

    #[cfg(target_os = "macos")]
    #[nolink]
    pub mod extra {
        use libc::types::os::arch::c95::{c_char, c_int};

        extern {
            pub fn _NSGetExecutablePath(buf: *mut c_char, bufsize: *mut u32)
                                        -> c_int;
        }
    }

    #[cfg(target_os = "freebsd")]
    pub mod extra {
    }

    #[cfg(target_os = "linux")]
    #[cfg(target_os = "android")]
    pub mod extra {
    }


    #[cfg(target_os = "win32")]
    pub mod extra {

        pub mod kernel32 {
            use libc::types::os::arch::c95::{c_uint};
            use libc::types::os::arch::extra::{BOOL, DWORD, SIZE_T, HMODULE};
            use libc::types::os::arch::extra::{LPCWSTR, LPWSTR, LPCTSTR,
                                               LPTSTR, LPTCH, LPDWORD, LPVOID,
                                               LPCVOID};
            use libc::types::os::arch::extra::{LPSECURITY_ATTRIBUTES, LPSTARTUPINFO,
                                               LPPROCESS_INFORMATION,
                                               LPMEMORY_BASIC_INFORMATION,
                                               LPSYSTEM_INFO};
            use libc::types::os::arch::extra::{HANDLE, LPHANDLE};

            #[cfg(target_arch = "x86")]
            extern "stdcall" {
                pub fn GetEnvironmentVariableW(n: LPCWSTR,
                                               v: LPWSTR,
                                               nsize: DWORD)
                                               -> DWORD;
                pub fn SetEnvironmentVariableW(n: LPCWSTR, v: LPCWSTR)
                                               -> BOOL;
                pub fn GetEnvironmentStringsA() -> LPTCH;
                pub fn FreeEnvironmentStringsA(env_ptr: LPTCH) -> BOOL;
                pub fn GetModuleFileNameW(hModule: HMODULE,
                                          lpFilename: LPWSTR,
                                          nSize: DWORD)
                                          -> DWORD;
                pub fn CreateDirectoryW(lpPathName: LPCWSTR,
                                        lpSecurityAttributes:
                                        LPSECURITY_ATTRIBUTES)
                                        -> BOOL;
                pub fn CopyFileW(lpExistingFileName: LPCWSTR,
                                 lpNewFileName: LPCWSTR,
                                 bFailIfExists: BOOL)
                                 -> BOOL;
                pub fn DeleteFileW(lpPathName: LPCWSTR) -> BOOL;
                pub fn RemoveDirectoryW(lpPathName: LPCWSTR) -> BOOL;
                pub fn GetCurrentDirectoryW(nBufferLength: DWORD,
                                            lpBuffer: LPWSTR)
                                            -> DWORD;
                pub fn SetCurrentDirectoryW(lpPathName: LPCWSTR) -> BOOL;
                pub fn GetLastError() -> DWORD;
                pub fn FindFirstFileW(fileName: *u16, findFileData: HANDLE)
                                      -> HANDLE;
                pub fn FindNextFileW(findFile: HANDLE, findFileData: HANDLE)
                                     -> BOOL;
                pub fn FindClose(findFile: HANDLE) -> BOOL;
                pub fn DuplicateHandle(hSourceProcessHandle: HANDLE,
                                       hSourceHandle: HANDLE,
                                       hTargetProcessHandle: HANDLE,
                                       lpTargetHandle: LPHANDLE,
                                       dwDesiredAccess: DWORD,
                                       bInheritHandle: BOOL,
                                       dwOptions: DWORD)
                                       -> BOOL;
                pub fn CloseHandle(hObject: HANDLE) -> BOOL;
                pub fn OpenProcess(dwDesiredAccess: DWORD,
                                   bInheritHandle: BOOL,
                                   dwProcessId: DWORD)
                                   -> HANDLE;
                pub fn GetCurrentProcess() -> HANDLE;
                pub fn CreateProcessA(lpApplicationName: LPCTSTR,
                                      lpCommandLine: LPTSTR,
                                      lpProcessAttributes:
                                      LPSECURITY_ATTRIBUTES,
                                      lpThreadAttributes:
                                      LPSECURITY_ATTRIBUTES,
                                      bInheritHandles: BOOL,
                                      dwCreationFlags: DWORD,
                                      lpEnvironment: LPVOID,
                                      lpCurrentDirectory: LPCTSTR,
                                      lpStartupInfo: LPSTARTUPINFO,
                                      lpProcessInformation:
                                      LPPROCESS_INFORMATION)
                                      -> BOOL;
                pub fn WaitForSingleObject(hHandle: HANDLE,
                                           dwMilliseconds: DWORD)
                                           -> DWORD;
                pub fn TerminateProcess(hProcess: HANDLE, uExitCode: c_uint)
                                        -> BOOL;
                pub fn GetExitCodeProcess(hProcess: HANDLE,
                                          lpExitCode: LPDWORD)
                                          -> BOOL;
                pub fn GetSystemInfo(lpSystemInfo: LPSYSTEM_INFO);
                pub fn VirtualAlloc(lpAddress: LPVOID,
                                    dwSize: SIZE_T,
                                    flAllocationType: DWORD,
                                    flProtect: DWORD)
                                    -> LPVOID;
                pub fn VirtualFree(lpAddress: LPVOID,
                                   dwSize: SIZE_T,
                                   dwFreeType: DWORD)
                                   -> BOOL;
                pub fn VirtualLock(lpAddress: LPVOID, dwSize: SIZE_T) -> BOOL;
                pub fn VirtualUnlock(lpAddress: LPVOID, dwSize: SIZE_T)
                                     -> BOOL;
                pub fn VirtualProtect(lpAddress: LPVOID,
                                      dwSize: SIZE_T,
                                      flNewProtect: DWORD,
                                      lpflOldProtect: LPDWORD)
                                      -> BOOL;
                pub fn VirtualQuery(lpAddress: LPCVOID,
                                    lpBuffer: LPMEMORY_BASIC_INFORMATION,
                                    dwLength: SIZE_T)
                                    -> SIZE_T;
                pub fn CreateFileMappingW(hFile: HANDLE,
                                          lpAttributes: LPSECURITY_ATTRIBUTES,
                                          flProtect: DWORD,
                                          dwMaximumSizeHigh: DWORD,
                                          dwMaximumSizeLow: DWORD,
                                          lpName: LPCTSTR)
                                          -> HANDLE;
                pub fn MapViewOfFile(hFileMappingObject: HANDLE,
                                     dwDesiredAccess: DWORD,
                                     dwFileOffsetHigh: DWORD,
                                     dwFileOffsetLow: DWORD,
                                     dwNumberOfBytesToMap: SIZE_T)
                                     -> LPVOID;
                pub fn UnmapViewOfFile(lpBaseAddress: LPCVOID) -> BOOL;
            }

            #[cfg(target_arch = "x86_64")]
            extern {
                pub fn GetEnvironmentVariableW(n: LPCWSTR,
                                               v: LPWSTR,
                                               nsize: DWORD)
                                               -> DWORD;
                pub fn SetEnvironmentVariableW(n: LPCWSTR, v: LPCWSTR)
                                               -> BOOL;
                pub fn GetEnvironmentStringsA() -> LPTCH;
                pub fn FreeEnvironmentStringsA(env_ptr: LPTCH) -> BOOL;
                pub fn GetModuleFileNameW(hModule: HMODULE,
                                          lpFilename: LPWSTR,
                                          nSize: DWORD)
                                          -> DWORD;
                pub fn CreateDirectoryW(lpPathName: LPCWSTR,
                                        lpSecurityAttributes:
                                        LPSECURITY_ATTRIBUTES)
                                        -> BOOL;
                pub fn CopyFileW(lpExistingFileName: LPCWSTR,
                                        lpNewFileName: LPCWSTR,
                                        bFailIfExists: BOOL)
                                        -> BOOL;
                pub fn DeleteFileW(lpPathName: LPCWSTR) -> BOOL;
                pub fn RemoveDirectoryW(lpPathName: LPCWSTR) -> BOOL;
                pub fn GetCurrentDirectoryW(nBufferLength: DWORD,
                                            lpBuffer: LPWSTR)
                                            -> DWORD;
                pub fn SetCurrentDirectoryW(lpPathName: LPCWSTR) -> BOOL;
                pub fn GetLastError() -> DWORD;
                pub fn FindFirstFileW(fileName: *u16, findFileData: HANDLE)
                                      -> HANDLE;
                pub fn FindNextFileW(findFile: HANDLE, findFileData: HANDLE)
                                     -> BOOL;
                pub fn FindClose(findFile: HANDLE) -> BOOL;
                pub fn DuplicateHandle(hSourceProcessHandle: HANDLE,
                                       hSourceHandle: HANDLE,
                                       hTargetProcessHandle: HANDLE,
                                       lpTargetHandle: LPHANDLE,
                                       dwDesiredAccess: DWORD,
                                       bInheritHandle: BOOL,
                                       dwOptions: DWORD)
                                       -> BOOL;
                pub fn CloseHandle(hObject: HANDLE) -> BOOL;
                pub fn OpenProcess(dwDesiredAccess: DWORD,
                                   bInheritHandle: BOOL,
                                   dwProcessId: DWORD)
                                   -> HANDLE;
                pub fn GetCurrentProcess() -> HANDLE;
                pub fn CreateProcessA(lpApplicationName: LPCTSTR,
                                      lpCommandLine: LPTSTR,
                                      lpProcessAttributes:
                                      LPSECURITY_ATTRIBUTES,
                                      lpThreadAttributes:
                                      LPSECURITY_ATTRIBUTES,
                                      bInheritHandles: BOOL,
                                      dwCreationFlags: DWORD,
                                      lpEnvironment: LPVOID,
                                      lpCurrentDirectory: LPCTSTR,
                                      lpStartupInfo: LPSTARTUPINFO,
                                      lpProcessInformation:
                                      LPPROCESS_INFORMATION)
                                      -> BOOL;
                pub fn WaitForSingleObject(hHandle: HANDLE,
                                           dwMilliseconds: DWORD)
                                           -> DWORD;
                pub fn TerminateProcess(hProcess: HANDLE, uExitCode: c_uint)
                                        -> BOOL;
                pub fn GetExitCodeProcess(hProcess: HANDLE,
                                          lpExitCode: LPDWORD)
                                          -> BOOL;
                pub fn GetSystemInfo(lpSystemInfo: LPSYSTEM_INFO);
                pub fn VirtualAlloc(lpAddress: LPVOID,
                                    dwSize: SIZE_T,
                                    flAllocationType: DWORD,
                                    flProtect: DWORD)
                                    -> LPVOID;
                pub fn VirtualFree(lpAddress: LPVOID,
                                   dwSize: SIZE_T,
                                   dwFreeType: DWORD)
                                   -> BOOL;
                pub fn VirtualLock(lpAddress: LPVOID, dwSize: SIZE_T) -> BOOL;
                pub fn VirtualUnlock(lpAddress: LPVOID, dwSize: SIZE_T)
                                     -> BOOL;
                pub fn VirtualProtect(lpAddress: LPVOID,
                                      dwSize: SIZE_T,
                                      flNewProtect: DWORD,
                                      lpflOldProtect: LPDWORD)
                                      -> BOOL;
                pub fn VirtualQuery(lpAddress: LPCVOID,
                                    lpBuffer: LPMEMORY_BASIC_INFORMATION,
                                    dwLength: SIZE_T)
                                    -> SIZE_T;
                pub fn CreateFileMappingW(hFile: HANDLE,
                                          lpAttributes: LPSECURITY_ATTRIBUTES,
                                          flProtect: DWORD,
                                          dwMaximumSizeHigh: DWORD,
                                          dwMaximumSizeLow: DWORD,
                                          lpName: LPCTSTR)
                                          -> HANDLE;
                pub fn MapViewOfFile(hFileMappingObject: HANDLE,
                                     dwDesiredAccess: DWORD,
                                     dwFileOffsetHigh: DWORD,
                                     dwFileOffsetLow: DWORD,
                                     dwNumberOfBytesToMap: SIZE_T)
                                     -> LPVOID;
                pub fn UnmapViewOfFile(lpBaseAddress: LPCVOID) -> BOOL;
            }
        }

        pub mod msvcrt {
            use libc::types::os::arch::c95::{c_int, c_long};

            #[nolink]
            extern {
                #[link_name = "_commit"]
                pub fn commit(fd: c_int) -> c_int;

                #[link_name = "_get_osfhandle"]
                pub fn get_osfhandle(fd: c_int) -> c_long;
            }
        }
    }
}
