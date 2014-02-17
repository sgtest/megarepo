// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::io;
use std::libc::{pid_t, c_void, c_int};
use std::libc;
use std::os;
use std::ptr;
use std::rt::rtio;
use p = std::io::process;

use super::IoResult;
use super::file;

#[cfg(windows)] use std::cast;
#[cfg(not(windows))] use super::retry;

/**
 * A value representing a child process.
 *
 * The lifetime of this value is linked to the lifetime of the actual
 * process - the Process destructor calls self.finish() which waits
 * for the process to terminate.
 */
pub struct Process {
    /// The unique id of the process (this should never be negative).
    priv pid: pid_t,

    /// A handle to the process - on unix this will always be NULL, but on
    /// windows it will be a HANDLE to the process, which will prevent the
    /// pid being re-used until the handle is closed.
    priv handle: *(),

    /// None until finish() is called.
    priv exit_code: Option<p::ProcessExit>,
}

impl Process {
    /// Creates a new process using native process-spawning abilities provided
    /// by the OS. Operations on this process will be blocking instead of using
    /// the runtime for sleeping just this current task.
    ///
    /// # Arguments
    ///
    /// * prog - the program to run
    /// * args - the arguments to pass to the program, not including the program
    ///          itself
    /// * env - an optional environment to specify for the child process. If
    ///         this value is `None`, then the child will inherit the parent's
    ///         environment
    /// * cwd - an optionally specified current working directory of the child,
    ///         defaulting to the parent's current working directory
    /// * stdin, stdout, stderr - These optionally specified file descriptors
    ///     dictate where the stdin/out/err of the child process will go. If
    ///     these are `None`, then this module will bind the input/output to an
    ///     os pipe instead. This process takes ownership of these file
    ///     descriptors, closing them upon destruction of the process.
    pub fn spawn(config: p::ProcessConfig)
        -> Result<(Process, ~[Option<file::FileDesc>]), io::IoError>
    {
        // right now we only handle stdin/stdout/stderr.
        if config.io.len() > 3 {
            return Err(super::unimpl());
        }

        fn get_io(io: &[p::StdioContainer],
                  ret: &mut ~[Option<file::FileDesc>],
                  idx: uint) -> (Option<os::Pipe>, c_int) {
            if idx >= io.len() { return (None, -1); }
            ret.push(None);
            match io[idx] {
                p::Ignored => (None, -1),
                p::InheritFd(fd) => (None, fd),
                p::CreatePipe(readable, _writable) => {
                    let pipe = os::pipe();
                    let (theirs, ours) = if readable {
                        (pipe.input, pipe.out)
                    } else {
                        (pipe.out, pipe.input)
                    };
                    ret[idx] = Some(file::FileDesc::new(ours, true));
                    (Some(pipe), theirs)
                }
            }
        }

        let mut ret_io = ~[];
        let (in_pipe, in_fd) = get_io(config.io, &mut ret_io, 0);
        let (out_pipe, out_fd) = get_io(config.io, &mut ret_io, 1);
        let (err_pipe, err_fd) = get_io(config.io, &mut ret_io, 2);

        let env = config.env.map(|a| a.to_owned());
        let cwd = config.cwd.map(|a| Path::new(a));
        let res = spawn_process_os(config, env, cwd.as_ref(), in_fd, out_fd,
                                   err_fd);

        unsafe {
            for pipe in in_pipe.iter() { let _ = libc::close(pipe.input); }
            for pipe in out_pipe.iter() { let _ = libc::close(pipe.out); }
            for pipe in err_pipe.iter() { let _ = libc::close(pipe.out); }
        }

        match res {
            Ok(res) => {
                Ok((Process { pid: res.pid, handle: res.handle, exit_code: None },
                    ret_io))
            }
            Err(e) => Err(e)
        }
    }
}

impl rtio::RtioProcess for Process {
    fn id(&self) -> pid_t { self.pid }

    fn wait(&mut self) -> p::ProcessExit {
        match self.exit_code {
            Some(code) => code,
            None => {
                let code = waitpid(self.pid);
                self.exit_code = Some(code);
                code
            }
        }
    }

    fn kill(&mut self, signum: int) -> Result<(), io::IoError> {
        // if the process has finished, and therefore had waitpid called,
        // and we kill it, then on unix we might ending up killing a
        // newer process that happens to have the same (re-used) id
        match self.exit_code {
            Some(..) => return Err(io::IoError {
                kind: io::OtherIoError,
                desc: "can't kill an exited process",
                detail: None,
            }),
            None => {}
        }
        return unsafe { killpid(self.pid, signum) };

        #[cfg(windows)]
        unsafe fn killpid(pid: pid_t, signal: int) -> Result<(), io::IoError> {
            match signal {
                io::process::PleaseExitSignal | io::process::MustDieSignal => {
                    let ret = libc::TerminateProcess(pid as libc::HANDLE, 1);
                    super::mkerr_winbool(ret)
                }
                _ => Err(io::IoError {
                    kind: io::OtherIoError,
                    desc: "unsupported signal on windows",
                    detail: None,
                })
            }
        }

        #[cfg(not(windows))]
        unsafe fn killpid(pid: pid_t, signal: int) -> Result<(), io::IoError> {
            let r = libc::funcs::posix88::signal::kill(pid, signal as c_int);
            super::mkerr_libc(r)
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        free_handle(self.handle);
    }
}

struct SpawnProcessResult {
    pid: pid_t,
    handle: *(),
}

#[cfg(windows)]
fn spawn_process_os(config: p::ProcessConfig,
                    env: Option<~[(~str, ~str)]>,
                    dir: Option<&Path>,
                    in_fd: c_int, out_fd: c_int,
                    err_fd: c_int) -> IoResult<SpawnProcessResult> {
    use std::libc::types::os::arch::extra::{DWORD, HANDLE, STARTUPINFO};
    use std::libc::consts::os::extra::{
        TRUE, FALSE,
        STARTF_USESTDHANDLES,
        INVALID_HANDLE_VALUE,
        DUPLICATE_SAME_ACCESS
    };
    use std::libc::funcs::extra::kernel32::{
        GetCurrentProcess,
        DuplicateHandle,
        CloseHandle,
        CreateProcessA
    };
    use std::libc::funcs::extra::msvcrt::get_osfhandle;

    use std::mem;

    if config.gid.is_some() || config.uid.is_some() {
        return Err(io::IoError {
            kind: io::OtherIoError,
            desc: "unsupported gid/uid requested on windows",
            detail: None,
        })
    }

    unsafe {

        let mut si = zeroed_startupinfo();
        si.cb = mem::size_of::<STARTUPINFO>() as DWORD;
        si.dwFlags = STARTF_USESTDHANDLES;

        let cur_proc = GetCurrentProcess();

        let orig_std_in = get_osfhandle(in_fd) as HANDLE;
        if orig_std_in == INVALID_HANDLE_VALUE as HANDLE {
            fail!("failure in get_osfhandle: {}", os::last_os_error());
        }
        if DuplicateHandle(cur_proc, orig_std_in, cur_proc, &mut si.hStdInput,
                           0, TRUE, DUPLICATE_SAME_ACCESS) == FALSE {
            fail!("failure in DuplicateHandle: {}", os::last_os_error());
        }

        let orig_std_out = get_osfhandle(out_fd) as HANDLE;
        if orig_std_out == INVALID_HANDLE_VALUE as HANDLE {
            fail!("failure in get_osfhandle: {}", os::last_os_error());
        }
        if DuplicateHandle(cur_proc, orig_std_out, cur_proc, &mut si.hStdOutput,
                           0, TRUE, DUPLICATE_SAME_ACCESS) == FALSE {
            fail!("failure in DuplicateHandle: {}", os::last_os_error());
        }

        let orig_std_err = get_osfhandle(err_fd) as HANDLE;
        if orig_std_err == INVALID_HANDLE_VALUE as HANDLE {
            fail!("failure in get_osfhandle: {}", os::last_os_error());
        }
        if DuplicateHandle(cur_proc, orig_std_err, cur_proc, &mut si.hStdError,
                           0, TRUE, DUPLICATE_SAME_ACCESS) == FALSE {
            fail!("failure in DuplicateHandle: {}", os::last_os_error());
        }

        let cmd = make_command_line(config.program, config.args);
        let mut pi = zeroed_process_information();
        let mut create_err = None;

        // stolen from the libuv code.
        let mut flags = 0;
        if config.detach {
            flags |= libc::DETACHED_PROCESS | libc::CREATE_NEW_PROCESS_GROUP;
        }

        with_envp(env, |envp| {
            with_dirp(dir, |dirp| {
                cmd.with_c_str(|cmdp| {
                    let created = CreateProcessA(ptr::null(), cast::transmute(cmdp),
                                                 ptr::mut_null(), ptr::mut_null(), TRUE,
                                                 flags, envp, dirp, &mut si,
                                                 &mut pi);
                    if created == FALSE {
                        create_err = Some(super::last_error());
                    }
                })
            })
        });

        assert!(CloseHandle(si.hStdInput) != 0);
        assert!(CloseHandle(si.hStdOutput) != 0);
        assert!(CloseHandle(si.hStdError) != 0);

        match create_err {
            Some(err) => return Err(err),
            None => {}
        }

        // We close the thread handle because we don't care about keeping the
        // thread id valid, and we aren't keeping the thread handle around to be
        // able to close it later. We don't close the process handle however
        // because std::we want the process id to stay valid at least until the
        // calling code closes the process handle.
        assert!(CloseHandle(pi.hThread) != 0);

        Ok(SpawnProcessResult {
            pid: pi.dwProcessId as pid_t,
            handle: pi.hProcess as *()
        })
    }
}

#[cfg(windows)]
fn zeroed_startupinfo() -> libc::types::os::arch::extra::STARTUPINFO {
    libc::types::os::arch::extra::STARTUPINFO {
        cb: 0,
        lpReserved: ptr::mut_null(),
        lpDesktop: ptr::mut_null(),
        lpTitle: ptr::mut_null(),
        dwX: 0,
        dwY: 0,
        dwXSize: 0,
        dwYSize: 0,
        dwXCountChars: 0,
        dwYCountCharts: 0,
        dwFillAttribute: 0,
        dwFlags: 0,
        wShowWindow: 0,
        cbReserved2: 0,
        lpReserved2: ptr::mut_null(),
        hStdInput: ptr::mut_null(),
        hStdOutput: ptr::mut_null(),
        hStdError: ptr::mut_null()
    }
}

#[cfg(windows)]
fn zeroed_process_information() -> libc::types::os::arch::extra::PROCESS_INFORMATION {
    libc::types::os::arch::extra::PROCESS_INFORMATION {
        hProcess: ptr::mut_null(),
        hThread: ptr::mut_null(),
        dwProcessId: 0,
        dwThreadId: 0
    }
}

#[cfg(windows)]
fn make_command_line(prog: &str, args: &[~str]) -> ~str {
    let mut cmd = ~"";
    append_arg(&mut cmd, prog);
    for arg in args.iter() {
        cmd.push_char(' ');
        append_arg(&mut cmd, *arg);
    }
    return cmd;

    fn append_arg(cmd: &mut ~str, arg: &str) {
        let quote = arg.chars().any(|c| c == ' ' || c == '\t');
        if quote {
            cmd.push_char('"');
        }
        for i in range(0u, arg.len()) {
            append_char_at(cmd, arg, i);
        }
        if quote {
            cmd.push_char('"');
        }
    }

    fn append_char_at(cmd: &mut ~str, arg: &str, i: uint) {
        match arg[i] as char {
            '"' => {
                // Escape quotes.
                cmd.push_str("\\\"");
            }
            '\\' => {
                if backslash_run_ends_in_quote(arg, i) {
                    // Double all backslashes that are in runs before quotes.
                    cmd.push_str("\\\\");
                } else {
                    // Pass other backslashes through unescaped.
                    cmd.push_char('\\');
                }
            }
            c => {
                cmd.push_char(c);
            }
        }
    }

    fn backslash_run_ends_in_quote(s: &str, mut i: uint) -> bool {
        while i < s.len() && s[i] as char == '\\' {
            i += 1;
        }
        return i < s.len() && s[i] as char == '"';
    }
}

#[cfg(unix)]
fn spawn_process_os(config: p::ProcessConfig,
                    env: Option<~[(~str, ~str)]>,
                    dir: Option<&Path>,
                    in_fd: c_int, out_fd: c_int,
                    err_fd: c_int) -> IoResult<SpawnProcessResult> {
    use std::libc::funcs::posix88::unistd::{fork, dup2, close, chdir, execvp};
    use std::libc::funcs::bsd44::getdtablesize;
    use std::libc::c_ulong;

    mod rustrt {
        extern {
            pub fn rust_unset_sigprocmask();
        }
    }

    #[cfg(target_os = "macos")]
    unsafe fn set_environ(envp: *c_void) {
        extern { fn _NSGetEnviron() -> *mut *c_void; }

        *_NSGetEnviron() = envp;
    }
    #[cfg(not(target_os = "macos"))]
    unsafe fn set_environ(envp: *c_void) {
        extern { static mut environ: *c_void; }
        environ = envp;
    }

    unsafe fn set_cloexec(fd: c_int) {
        extern { fn ioctl(fd: c_int, req: c_ulong) -> c_int; }

        #[cfg(target_os = "macos")]
        #[cfg(target_os = "freebsd")]
        static FIOCLEX: c_ulong = 0x20006601;
        #[cfg(target_os = "linux")]
        #[cfg(target_os = "android")]
        static FIOCLEX: c_ulong = 0x5451;

        let ret = ioctl(fd, FIOCLEX);
        assert_eq!(ret, 0);
    }

    let pipe = os::pipe();
    let mut input = file::FileDesc::new(pipe.input, true);
    let mut output = file::FileDesc::new(pipe.out, true);

    unsafe { set_cloexec(output.fd()) };

    unsafe {
        let pid = fork();
        if pid < 0 {
            fail!("failure in fork: {}", os::last_os_error());
        } else if pid > 0 {
            drop(output);
            let mut bytes = [0, ..4];
            return match input.inner_read(bytes) {
                Ok(4) => {
                    let errno = (bytes[0] << 24) as i32 |
                                (bytes[1] << 16) as i32 |
                                (bytes[2] <<  8) as i32 |
                                (bytes[3] <<  0) as i32;
                    Err(super::translate_error(errno, false))
                }
                Err(e) => {
                    assert!(e.kind == io::BrokenPipe ||
                            e.kind == io::EndOfFile,
                            "unexpected error: {:?}", e);
                    Ok(SpawnProcessResult {
                        pid: pid,
                        handle: ptr::null()
                    })
                }
                Ok(..) => fail!("short read on the cloexec pipe"),
            };
        }
        drop(input);

        fn fail(output: &mut file::FileDesc) -> ! {
            let errno = os::errno();
            let bytes = [
                (errno << 24) as u8,
                (errno << 16) as u8,
                (errno <<  8) as u8,
                (errno <<  0) as u8,
            ];
            assert!(output.inner_write(bytes).is_ok());
            unsafe { libc::_exit(1) }
        }

        rustrt::rust_unset_sigprocmask();

        if in_fd == -1 {
            let _ = libc::close(libc::STDIN_FILENO);
        } else if retry(|| dup2(in_fd, 0)) == -1 {
            fail(&mut output);
        }
        if out_fd == -1 {
            let _ = libc::close(libc::STDOUT_FILENO);
        } else if retry(|| dup2(out_fd, 1)) == -1 {
            fail(&mut output);
        }
        if err_fd == -1 {
            let _ = libc::close(libc::STDERR_FILENO);
        } else if retry(|| dup2(err_fd, 2)) == -1 {
            fail(&mut output);
        }
        // close all other fds
        for fd in range(3, getdtablesize()).rev() {
            if fd != output.fd() {
                let _ = close(fd as c_int);
            }
        }

        match config.gid {
            Some(u) => {
                if libc::setgid(u as libc::gid_t) != 0 {
                    fail(&mut output);
                }
            }
            None => {}
        }
        match config.uid {
            Some(u) => {
                // When dropping privileges from root, the `setgroups` call will
                // remove any extraneous groups. If we don't call this, then
                // even though our uid has dropped, we may still have groups
                // that enable us to do super-user things. This will fail if we
                // aren't root, so don't bother checking the return value, this
                // is just done as an optimistic privilege dropping function.
                extern {
                    fn setgroups(ngroups: libc::c_int,
                                 ptr: *libc::c_void) -> libc::c_int;
                }
                let _ = setgroups(0, 0 as *libc::c_void);

                if libc::setuid(u as libc::uid_t) != 0 {
                    fail(&mut output);
                }
            }
            None => {}
        }
        if config.detach {
            // Don't check the error of setsid because it fails if we're the
            // process leader already. We just forked so it shouldn't return
            // error, but ignore it anyway.
            let _ = libc::setsid();
        }

        with_dirp(dir, |dirp| {
            if !dirp.is_null() && chdir(dirp) == -1 {
                fail(&mut output);
            }
        });

        with_envp(env, |envp| {
            if !envp.is_null() {
                set_environ(envp);
            }
        });
        with_argv(config.program, config.args, |argv| {
            let _ = execvp(*argv, argv);
            fail(&mut output);
        })
    }
}

#[cfg(unix)]
fn with_argv<T>(prog: &str, args: &[~str], cb: |**libc::c_char| -> T) -> T {
    use std::vec;

    // We can't directly convert `str`s into `*char`s, as someone needs to hold
    // a reference to the intermediary byte buffers. So first build an array to
    // hold all the ~[u8] byte strings.
    let mut tmps = vec::with_capacity(args.len() + 1);

    tmps.push(prog.to_c_str());

    for arg in args.iter() {
        tmps.push(arg.to_c_str());
    }

    // Next, convert each of the byte strings into a pointer. This is
    // technically unsafe as the caller could leak these pointers out of our
    // scope.
    let mut ptrs = tmps.map(|tmp| tmp.with_ref(|buf| buf));

    // Finally, make sure we add a null pointer.
    ptrs.push(ptr::null());

    cb(ptrs.as_ptr())
}

#[cfg(unix)]
fn with_envp<T>(env: Option<~[(~str, ~str)]>, cb: |*c_void| -> T) -> T {
    use std::vec;

    // On posixy systems we can pass a char** for envp, which is a
    // null-terminated array of "k=v\n" strings. Like `with_argv`, we have to
    // have a temporary buffer to hold the intermediary `~[u8]` byte strings.
    match env {
        Some(env) => {
            let mut tmps = vec::with_capacity(env.len());

            for pair in env.iter() {
                let kv = format!("{}={}", *pair.ref0(), *pair.ref1());
                tmps.push(kv.to_c_str());
            }

            // Once again, this is unsafe.
            let mut ptrs = tmps.map(|tmp| tmp.with_ref(|buf| buf));
            ptrs.push(ptr::null());

            cb(ptrs.as_ptr() as *c_void)
        }
        _ => cb(ptr::null())
    }
}

#[cfg(windows)]
fn with_envp<T>(env: Option<~[(~str, ~str)]>, cb: |*mut c_void| -> T) -> T {
    // On win32 we pass an "environment block" which is not a char**, but
    // rather a concatenation of null-terminated k=v\0 sequences, with a final
    // \0 to terminate.
    match env {
        Some(env) => {
            let mut blk = ~[];

            for pair in env.iter() {
                let kv = format!("{}={}", *pair.ref0(), *pair.ref1());
                blk.push_all(kv.as_bytes());
                blk.push(0);
            }

            blk.push(0);

            cb(blk.as_mut_ptr() as *mut c_void)
        }
        _ => cb(ptr::mut_null())
    }
}

fn with_dirp<T>(d: Option<&Path>, cb: |*libc::c_char| -> T) -> T {
    match d {
      Some(dir) => dir.with_c_str(|buf| cb(buf)),
      None => cb(ptr::null())
    }
}

#[cfg(windows)]
fn free_handle(handle: *()) {
    assert!(unsafe {
        libc::CloseHandle(cast::transmute(handle)) != 0
    })
}

#[cfg(unix)]
fn free_handle(_handle: *()) {
    // unix has no process handle object, just a pid
}

/**
 * Waits for a process to exit and returns the exit code, failing
 * if there is no process with the specified id.
 *
 * Note that this is private to avoid race conditions on unix where if
 * a user calls waitpid(some_process.get_id()) then some_process.finish()
 * and some_process.destroy() and some_process.finalize() will then either
 * operate on a none-existent process or, even worse, on a newer process
 * with the same id.
 */
fn waitpid(pid: pid_t) -> p::ProcessExit {
    return waitpid_os(pid);

    #[cfg(windows)]
    fn waitpid_os(pid: pid_t) -> p::ProcessExit {
        use std::libc::types::os::arch::extra::DWORD;
        use std::libc::consts::os::extra::{
            SYNCHRONIZE,
            PROCESS_QUERY_INFORMATION,
            FALSE,
            STILL_ACTIVE,
            INFINITE,
            WAIT_FAILED
        };
        use std::libc::funcs::extra::kernel32::{
            OpenProcess,
            GetExitCodeProcess,
            CloseHandle,
            WaitForSingleObject
        };

        unsafe {

            let process = OpenProcess(SYNCHRONIZE | PROCESS_QUERY_INFORMATION,
                                      FALSE,
                                      pid as DWORD);
            if process.is_null() {
                fail!("failure in OpenProcess: {}", os::last_os_error());
            }

            loop {
                let mut status = 0;
                if GetExitCodeProcess(process, &mut status) == FALSE {
                    assert!(CloseHandle(process) != 0);
                    fail!("failure in GetExitCodeProcess: {}", os::last_os_error());
                }
                if status != STILL_ACTIVE {
                    assert!(CloseHandle(process) != 0);
                    return p::ExitStatus(status as int);
                }
                if WaitForSingleObject(process, INFINITE) == WAIT_FAILED {
                    assert!(CloseHandle(process) != 0);
                    fail!("failure in WaitForSingleObject: {}", os::last_os_error());
                }
            }
        }
    }

    #[cfg(unix)]
    fn waitpid_os(pid: pid_t) -> p::ProcessExit {
        use std::libc::funcs::posix01::wait;

        #[cfg(target_os = "linux")]
        #[cfg(target_os = "android")]
        mod imp {
            pub fn WIFEXITED(status: i32) -> bool { (status & 0xff) == 0 }
            pub fn WEXITSTATUS(status: i32) -> i32 { (status >> 8) & 0xff }
            pub fn WTERMSIG(status: i32) -> i32 { status & 0x7f }
        }

        #[cfg(target_os = "macos")]
        #[cfg(target_os = "freebsd")]
        mod imp {
            pub fn WIFEXITED(status: i32) -> bool { (status & 0x7f) == 0 }
            pub fn WEXITSTATUS(status: i32) -> i32 { status >> 8 }
            pub fn WTERMSIG(status: i32) -> i32 { status & 0o177 }
        }

        let mut status = 0 as c_int;
        match retry(|| unsafe { wait::waitpid(pid, &mut status, 0) }) {
            -1 => fail!("unknown waitpid error: {:?}", super::last_error()),
            _ => {
                if imp::WIFEXITED(status) {
                    p::ExitStatus(imp::WEXITSTATUS(status) as int)
                } else {
                    p::ExitSignal(imp::WTERMSIG(status) as int)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {

    #[test] #[cfg(windows)]
    fn test_make_command_line() {
        use super::make_command_line;
        assert_eq!(
            make_command_line("prog", [~"aaa", ~"bbb", ~"ccc"]),
            ~"prog aaa bbb ccc"
        );
        assert_eq!(
            make_command_line("C:\\Program Files\\blah\\blah.exe", [~"aaa"]),
            ~"\"C:\\Program Files\\blah\\blah.exe\" aaa"
        );
        assert_eq!(
            make_command_line("C:\\Program Files\\test", [~"aa\"bb"]),
            ~"\"C:\\Program Files\\test\" aa\\\"bb"
        );
        assert_eq!(
            make_command_line("echo", [~"a b c"]),
            ~"echo \"a b c\""
        );
    }
}
