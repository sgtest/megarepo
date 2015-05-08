// Copyright 2014-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use prelude::v1::*;
use os::unix::prelude::*;

use collections::HashMap;
use env;
use ffi::{OsString, OsStr, CString, CStr};
use fmt;
use io::{self, Error, ErrorKind};
use libc::{self, pid_t, c_void, c_int, gid_t, uid_t};
use ptr;
use sys::pipe::AnonPipe;
use sys::{self, c, cvt, cvt_r};
use sys::fs::{File, OpenOptions};

////////////////////////////////////////////////////////////////////////////////
// Command
////////////////////////////////////////////////////////////////////////////////

#[derive(Clone)]
pub struct Command {
    pub program: CString,
    pub args: Vec<CString>,
    pub env: Option<HashMap<OsString, OsString>>,
    pub cwd: Option<CString>,
    pub uid: Option<uid_t>,
    pub gid: Option<gid_t>,
    pub detach: bool, // not currently exposed in std::process
}

impl Command {
    pub fn new(program: &OsStr) -> Command {
        Command {
            program: program.to_cstring().unwrap(),
            args: Vec::new(),
            env: None,
            cwd: None,
            uid: None,
            gid: None,
            detach: false,
        }
    }

    pub fn arg(&mut self, arg: &OsStr) {
        self.args.push(arg.to_cstring().unwrap())
    }
    pub fn args<'a, I: Iterator<Item = &'a OsStr>>(&mut self, args: I) {
        self.args.extend(args.map(|s| s.to_cstring().unwrap()))
    }
    fn init_env_map(&mut self) {
        if self.env.is_none() {
            self.env = Some(env::vars_os().collect());
        }
    }
    pub fn env(&mut self, key: &OsStr, val: &OsStr) {
        self.init_env_map();
        self.env.as_mut().unwrap().insert(key.to_os_string(), val.to_os_string());
    }
    pub fn env_remove(&mut self, key: &OsStr) {
        self.init_env_map();
        self.env.as_mut().unwrap().remove(&key.to_os_string());
    }
    pub fn env_clear(&mut self) {
        self.env = Some(HashMap::new())
    }
    pub fn cwd(&mut self, dir: &OsStr) {
        self.cwd = Some(dir.to_cstring().unwrap())
    }
}

////////////////////////////////////////////////////////////////////////////////
// Processes
////////////////////////////////////////////////////////////////////////////////

/// Unix exit statuses
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum ExitStatus {
    /// Normal termination with an exit code.
    Code(i32),

    /// Termination by signal, with the signal number.
    ///
    /// Never generated on Windows.
    Signal(i32),
}

impl ExitStatus {
    pub fn success(&self) -> bool {
        *self == ExitStatus::Code(0)
    }
    pub fn code(&self) -> Option<i32> {
        match *self {
            ExitStatus::Code(c) => Some(c),
            _ => None
        }
    }
}

impl fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ExitStatus::Code(code) =>  write!(f, "exit code: {}", code),
            ExitStatus::Signal(code) =>  write!(f, "signal: {}", code),
        }
    }
}

/// The unique id of the process (this should never be negative).
pub struct Process {
    pid: pid_t
}

pub enum Stdio {
    Inherit,
    Piped(AnonPipe),
    None,
}

const CLOEXEC_MSG_FOOTER: &'static [u8] = b"NOEX";

impl Process {
    pub unsafe fn kill(&self) -> io::Result<()> {
        try!(cvt(libc::funcs::posix88::signal::kill(self.pid, libc::SIGKILL)));
        Ok(())
    }

    pub fn spawn(cfg: &Command,
                 in_fd: Stdio,
                 out_fd: Stdio,
                 err_fd: Stdio) -> io::Result<Process> {
        let dirp = cfg.cwd.as_ref().map(|c| c.as_ptr()).unwrap_or(ptr::null());

        let (envp, _a, _b) = make_envp(cfg.env.as_ref());
        let (argv, _a) = make_argv(&cfg.program, &cfg.args);
        let (input, output) = try!(sys::pipe::anon_pipe());

        let pid = unsafe {
            match libc::fork() {
                0 => {
                    drop(input);
                    Process::child_after_fork(cfg, output, argv, envp, dirp,
                                              in_fd, out_fd, err_fd)
                }
                n if n < 0 => return Err(Error::last_os_error()),
                n => n,
            }
        };

        let p = Process{ pid: pid };
        drop(output);
        let mut bytes = [0; 8];

        // loop to handle EINTR
        loop {
            match input.read(&mut bytes) {
                Ok(0) => return Ok(p),
                Ok(8) => {
                    assert!(combine(CLOEXEC_MSG_FOOTER) == combine(&bytes[4.. 8]),
                            "Validation on the CLOEXEC pipe failed: {:?}", bytes);
                    let errno = combine(&bytes[0.. 4]);
                    assert!(p.wait().is_ok(),
                            "wait() should either return Ok or panic");
                    return Err(Error::from_raw_os_error(errno))
                }
                Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => {
                    assert!(p.wait().is_ok(),
                            "wait() should either return Ok or panic");
                    panic!("the CLOEXEC pipe failed: {:?}", e)
                },
                Ok(..) => { // pipe I/O up to PIPE_BUF bytes should be atomic
                    assert!(p.wait().is_ok(),
                            "wait() should either return Ok or panic");
                    panic!("short read on the CLOEXEC pipe")
                }
            }
        }

        fn combine(arr: &[u8]) -> i32 {
            let a = arr[0] as u32;
            let b = arr[1] as u32;
            let c = arr[2] as u32;
            let d = arr[3] as u32;

            ((a << 24) | (b << 16) | (c << 8) | (d << 0)) as i32
        }
    }

    // And at this point we've reached a special time in the life of the
    // child. The child must now be considered hamstrung and unable to
    // do anything other than syscalls really. Consider the following
    // scenario:
    //
    //      1. Thread A of process 1 grabs the malloc() mutex
    //      2. Thread B of process 1 forks(), creating thread C
    //      3. Thread C of process 2 then attempts to malloc()
    //      4. The memory of process 2 is the same as the memory of
    //         process 1, so the mutex is locked.
    //
    // This situation looks a lot like deadlock, right? It turns out
    // that this is what pthread_atfork() takes care of, which is
    // presumably implemented across platforms. The first thing that
    // threads to *before* forking is to do things like grab the malloc
    // mutex, and then after the fork they unlock it.
    //
    // Despite this information, libnative's spawn has been witnessed to
    // deadlock on both OSX and FreeBSD. I'm not entirely sure why, but
    // all collected backtraces point at malloc/free traffic in the
    // child spawned process.
    //
    // For this reason, the block of code below should contain 0
    // invocations of either malloc of free (or their related friends).
    //
    // As an example of not having malloc/free traffic, we don't close
    // this file descriptor by dropping the FileDesc (which contains an
    // allocation). Instead we just close it manually. This will never
    // have the drop glue anyway because this code never returns (the
    // child will either exec() or invoke libc::exit)
    unsafe fn child_after_fork(cfg: &Command,
                               mut output: AnonPipe,
                               argv: *const *const libc::c_char,
                               envp: *const libc::c_void,
                               dirp: *const libc::c_char,
                               in_fd: Stdio,
                               out_fd: Stdio,
                               err_fd: Stdio) -> ! {
        fn fail(output: &mut AnonPipe) -> ! {
            let errno = sys::os::errno() as u32;
            let bytes = [
                (errno >> 24) as u8,
                (errno >> 16) as u8,
                (errno >>  8) as u8,
                (errno >>  0) as u8,
                CLOEXEC_MSG_FOOTER[0], CLOEXEC_MSG_FOOTER[1],
                CLOEXEC_MSG_FOOTER[2], CLOEXEC_MSG_FOOTER[3]
            ];
            // pipe I/O up to PIPE_BUF bytes should be atomic, and then we want
            // to be sure we *don't* run at_exit destructors as we're being torn
            // down regardless
            assert!(output.write(&bytes).is_ok());
            unsafe { libc::_exit(1) }
        }

        let setup = |src: Stdio, dst: c_int| {
            let fd = match src {
                Stdio::Inherit => return true,
                Stdio::Piped(pipe) => pipe.into_fd(),

                // If a stdio file descriptor is set to be ignored, we open up
                // /dev/null into that file descriptor. Otherwise, the first
                // file descriptor opened up in the child would be numbered as
                // one of the stdio file descriptors, which is likely to wreak
                // havoc.
                Stdio::None => {
                    let mut opts = OpenOptions::new();
                    opts.read(dst == libc::STDIN_FILENO);
                    opts.write(dst != libc::STDIN_FILENO);
                    let devnull = CStr::from_ptr(b"/dev/null\0".as_ptr()
                                                    as *const _);
                    if let Ok(f) = File::open_c(devnull, &opts) {
                        f.into_fd()
                    } else {
                        return false
                    }
                }
            };
            cvt_r(|| libc::dup2(fd.raw(), dst)).is_ok()
        };

        if !setup(in_fd, libc::STDIN_FILENO) { fail(&mut output) }
        if !setup(out_fd, libc::STDOUT_FILENO) { fail(&mut output) }
        if !setup(err_fd, libc::STDERR_FILENO) { fail(&mut output) }

        if let Some(u) = cfg.gid {
            if libc::setgid(u as libc::gid_t) != 0 {
                fail(&mut output);
            }
        }
        if let Some(u) = cfg.uid {
            // When dropping privileges from root, the `setgroups` call
            // will remove any extraneous groups. If we don't call this,
            // then even though our uid has dropped, we may still have
            // groups that enable us to do super-user things. This will
            // fail if we aren't root, so don't bother checking the
            // return value, this is just done as an optimistic
            // privilege dropping function.
            let _ = c::setgroups(0, ptr::null());

            if libc::setuid(u as libc::uid_t) != 0 {
                fail(&mut output);
            }
        }
        if cfg.detach {
            // Don't check the error of setsid because it fails if we're the
            // process leader already. We just forked so it shouldn't return
            // error, but ignore it anyway.
            let _ = libc::setsid();
        }
        if !dirp.is_null() && libc::chdir(dirp) == -1 {
            fail(&mut output);
        }
        if !envp.is_null() {
            *sys::os::environ() = envp as *const _;
        }
        let _ = libc::execvp(*argv, argv as *mut _);
        fail(&mut output)
    }

    pub fn wait(&self) -> io::Result<ExitStatus> {
        let mut status = 0 as c_int;
        try!(cvt_r(|| unsafe { c::waitpid(self.pid, &mut status, 0) }));
        Ok(translate_status(status))
    }

    pub fn try_wait(&self) -> Option<ExitStatus> {
        let mut status = 0 as c_int;
        match cvt_r(|| unsafe {
            c::waitpid(self.pid, &mut status, c::WNOHANG)
        }) {
            Ok(0) => None,
            Ok(n) if n == self.pid => Some(translate_status(status)),
            Ok(n) => panic!("unknown pid: {}", n),
            Err(e) => panic!("unknown waitpid error: {}", e),
        }
    }
}

fn make_argv(prog: &CString, args: &[CString])
             -> (*const *const libc::c_char, Vec<*const libc::c_char>)
{
    let mut ptrs: Vec<*const libc::c_char> = Vec::with_capacity(args.len()+1);

    // Convert the CStrings into an array of pointers. Note: the
    // lifetime of the various CStrings involved is guaranteed to be
    // larger than the lifetime of our invocation of cb, but this is
    // technically unsafe as the callback could leak these pointers
    // out of our scope.
    ptrs.push(prog.as_ptr());
    ptrs.extend(args.iter().map(|tmp| tmp.as_ptr()));

    // Add a terminating null pointer (required by libc).
    ptrs.push(ptr::null());

    (ptrs.as_ptr(), ptrs)
}

fn make_envp(env: Option<&HashMap<OsString, OsString>>)
             -> (*const c_void, Vec<Vec<u8>>, Vec<*const libc::c_char>)
{
    // On posixy systems we can pass a char** for envp, which is a
    // null-terminated array of "k=v\0" strings. Since we must create
    // these strings locally, yet expose a raw pointer to them, we
    // create a temporary vector to own the CStrings that outlives the
    // call to cb.
    if let Some(env) = env {
        let mut tmps = Vec::with_capacity(env.len());

        for pair in env {
            let mut kv = Vec::new();
            kv.push_all(pair.0.as_bytes());
            kv.push('=' as u8);
            kv.push_all(pair.1.as_bytes());
            kv.push(0); // terminating null
            tmps.push(kv);
        }

        let mut ptrs: Vec<*const libc::c_char> =
            tmps.iter()
                .map(|tmp| tmp.as_ptr() as *const libc::c_char)
                .collect();
        ptrs.push(ptr::null());

        (ptrs.as_ptr() as *const _, tmps, ptrs)
    } else {
        (0 as *const _, Vec::new(), Vec::new())
    }
}

fn translate_status(status: c_int) -> ExitStatus {
    #![allow(non_snake_case)]
    #[cfg(any(target_os = "linux", target_os = "android"))]
    mod imp {
        pub fn WIFEXITED(status: i32) -> bool { (status & 0xff) == 0 }
        pub fn WEXITSTATUS(status: i32) -> i32 { (status >> 8) & 0xff }
        pub fn WTERMSIG(status: i32) -> i32 { status & 0x7f }
    }

    #[cfg(any(target_os = "macos",
              target_os = "ios",
              target_os = "freebsd",
              target_os = "dragonfly",
              target_os = "bitrig",
              target_os = "openbsd"))]
    mod imp {
        pub fn WIFEXITED(status: i32) -> bool { (status & 0x7f) == 0 }
        pub fn WEXITSTATUS(status: i32) -> i32 { status >> 8 }
        pub fn WTERMSIG(status: i32) -> i32 { status & 0o177 }
    }

    if imp::WIFEXITED(status) {
        ExitStatus::Code(imp::WEXITSTATUS(status))
    } else {
        ExitStatus::Signal(imp::WTERMSIG(status))
    }
}
