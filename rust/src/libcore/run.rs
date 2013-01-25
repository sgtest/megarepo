// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// NB: transitionary, de-mode-ing.
#[forbid(deprecated_mode)];
#[forbid(deprecated_pattern)];

//! Process spawning
use io;
use io::ReaderUtil;
use libc;
use libc::{pid_t, c_void, c_int};
use oldcomm;
use option::{Some, None};
use os;
use prelude::*;
use ptr;
use run;
use str;
use task;
use vec;

#[abi = "cdecl"]
extern mod rustrt {
    unsafe fn rust_run_program(argv: **libc::c_char, envp: *c_void,
                               dir: *libc::c_char,
                               in_fd: c_int, out_fd: c_int, err_fd: c_int)
                            -> pid_t;
}

/// A value representing a child process
pub trait Program {
    /// Returns the process id of the program
    fn get_id() -> pid_t;

    /// Returns an io::writer that can be used to write to stdin
    fn input() -> io::Writer;

    /// Returns an io::reader that can be used to read from stdout
    fn output() -> io::Reader;

    /// Returns an io::reader that can be used to read from stderr
    fn err() -> io::Reader;

    /// Closes the handle to the child processes standard input
    fn close_input();

    /**
     * Waits for the child process to terminate. Closes the handle
     * to stdin if necessary.
     */
    fn finish() -> int;

    /// Closes open handles
    fn destroy();
}


/**
 * Run a program, providing stdin, stdout and stderr handles
 *
 * # Arguments
 *
 * * prog - The path to an executable
 * * args - Vector of arguments to pass to the child process
 * * env - optional env-modification for child
 * * dir - optional dir to run child in (default current dir)
 * * in_fd - A file descriptor for the child to use as std input
 * * out_fd - A file descriptor for the child to use as std output
 * * err_fd - A file descriptor for the child to use as std error
 *
 * # Return value
 *
 * The process id of the spawned process
 */
pub fn spawn_process(prog: &str, args: &[~str],
                 env: &Option<~[(~str,~str)]>,
                 dir: &Option<~str>,
                 in_fd: c_int, out_fd: c_int, err_fd: c_int)
              -> pid_t {
    unsafe {
        do with_argv(prog, args) |argv| {
            do with_envp(env) |envp| {
                do with_dirp(dir) |dirp| {
                    rustrt::rust_run_program(argv, envp, dirp,
                                             in_fd, out_fd, err_fd)
                }
            }
        }
    }
}

fn with_argv<T>(prog: &str, args: &[~str],
                cb: fn(**libc::c_char) -> T) -> T {
    let mut argptrs = str::as_c_str(prog, |b| ~[b]);
    let mut tmps = ~[];
    for vec::each(args) |arg| {
        let t = @copy *arg;
        tmps.push(t);
        argptrs.push_all(str::as_c_str(*t, |b| ~[b]));
    }
    argptrs.push(ptr::null());
    vec::as_imm_buf(argptrs, |buf, _len| cb(buf))
}

#[cfg(unix)]
fn with_envp<T>(env: &Option<~[(~str,~str)]>,
                cb: fn(*c_void) -> T) -> T {
    // On posixy systems we can pass a char** for envp, which is
    // a null-terminated array of "k=v\n" strings.
    match *env {
      Some(ref es) if !vec::is_empty(*es) => {
        let mut tmps = ~[];
        let mut ptrs = ~[];

        for vec::each(*es) |e| {
            let (k,v) = copy *e;
            let t = @(fmt!("%s=%s", k, v));
            tmps.push(t);
            ptrs.push_all(str::as_c_str(*t, |b| ~[b]));
        }
        ptrs.push(ptr::null());
        vec::as_imm_buf(ptrs, |p, _len|
            unsafe { cb(::cast::reinterpret_cast(&p)) }
        )
      }
      _ => cb(ptr::null())
    }
}

#[cfg(windows)]
fn with_envp<T>(env: &Option<~[(~str,~str)]>,
                cb: fn(*c_void) -> T) -> T {
    // On win32 we pass an "environment block" which is not a char**, but
    // rather a concatenation of null-terminated k=v\0 sequences, with a final
    // \0 to terminate.
    unsafe {
        match *env {
          Some(ref es) if !vec::is_empty(*es) => {
            let mut blk : ~[u8] = ~[];
            for vec::each(*es) |e| {
                let (k,v) = copy *e;
                let t = fmt!("%s=%s", k, v);
                let mut v : ~[u8] = ::cast::reinterpret_cast(&t);
                blk += v;
                ::cast::forget(v);
            }
            blk += ~[0_u8];
            vec::as_imm_buf(blk, |p, _len| cb(::cast::reinterpret_cast(&p)))
          }
          _ => cb(ptr::null())
        }
    }
}

fn with_dirp<T>(d: &Option<~str>,
                cb: fn(*libc::c_char) -> T) -> T {
    match *d {
      Some(ref dir) => str::as_c_str(*dir, cb),
      None => cb(ptr::null())
    }
}

/**
 * Spawns a process and waits for it to terminate
 *
 * # Arguments
 *
 * * prog - The path to an executable
 * * args - Vector of arguments to pass to the child process
 *
 * # Return value
 *
 * The process id
 */
pub fn run_program(prog: &str, args: &[~str]) -> int {
    let pid = spawn_process(prog, args, &None, &None,
                            0i32, 0i32, 0i32);
    if pid == -1 as pid_t { fail; }
    return waitpid(pid);
}

/**
 * Spawns a process and returns a program
 *
 * The returned value is a boxed class containing a <program> object that can
 * be used for sending and receiving data over the standard file descriptors.
 * The class will ensure that file descriptors are closed properly.
 *
 * # Arguments
 *
 * * prog - The path to an executable
 * * args - Vector of arguments to pass to the child process
 *
 * # Return value
 *
 * A class with a <program> field
 */
pub fn start_program(prog: &str, args: &[~str]) -> Program {
    let pipe_input = os::pipe();
    let pipe_output = os::pipe();
    let pipe_err = os::pipe();
    let pid =
        spawn_process(prog, args, &None, &None,
                      pipe_input.in, pipe_output.out,
                      pipe_err.out);

    unsafe {
        if pid == -1 as pid_t { fail; }
        libc::close(pipe_input.in);
        libc::close(pipe_output.out);
        libc::close(pipe_err.out);
    }

    struct ProgRepr {
        pid: pid_t,
        mut in_fd: c_int,
        out_file: *libc::FILE,
        err_file: *libc::FILE,
        mut finished: bool,
    }

    fn close_repr_input(r: &ProgRepr) {
        let invalid_fd = -1i32;
        if r.in_fd != invalid_fd {
            unsafe {
                libc::close(r.in_fd);
            }
            r.in_fd = invalid_fd;
        }
    }
    fn finish_repr(r: &ProgRepr) -> int {
        if r.finished { return 0; }
        r.finished = true;
        close_repr_input(r);
        return waitpid(r.pid);
    }
    fn destroy_repr(r: &ProgRepr) {
        unsafe {
            finish_repr(r);
            libc::fclose(r.out_file);
            libc::fclose(r.err_file);
        }
    }
    struct ProgRes {
        r: ProgRepr,
        drop { destroy_repr(&self.r); }
    }

    fn ProgRes(r: ProgRepr) -> ProgRes {
        ProgRes {
            r: move r
        }
    }

    impl ProgRes: Program {
        fn get_id() -> pid_t { return self.r.pid; }
        fn input() -> io::Writer {
            io::fd_writer(self.r.in_fd, false)
        }
        fn output() -> io::Reader {
            io::FILE_reader(self.r.out_file, false)
        }
        fn err() -> io::Reader {
            io::FILE_reader(self.r.err_file, false)
        }
        fn close_input() { close_repr_input(&self.r); }
        fn finish() -> int { finish_repr(&self.r) }
        fn destroy() { destroy_repr(&self.r); }
    }
    let repr = ProgRepr {
        pid: pid,
        in_fd: pipe_input.out,
        out_file: os::fdopen(pipe_output.in),
        err_file: os::fdopen(pipe_err.in),
        finished: false,
    };

    ProgRes(repr) as Program
}

fn read_all(rd: io::Reader) -> ~str {
    let buf = io::with_bytes_writer(|wr| {
        let mut bytes = [mut 0, ..4096];
        while !rd.eof() {
            let nread = rd.read(bytes, bytes.len());
            wr.write(bytes.view(0, nread));
        }
    });
    str::from_bytes(buf)
}

/**
 * Spawns a process, waits for it to exit, and returns the exit code, and
 * contents of stdout and stderr.
 *
 * # Arguments
 *
 * * prog - The path to an executable
 * * args - Vector of arguments to pass to the child process
 *
 * # Return value
 *
 * A record, {status: int, out: str, err: str} containing the exit code,
 * the contents of stdout and the contents of stderr.
 */
pub fn program_output(prog: &str, args: &[~str]) ->
   {status: int, out: ~str, err: ~str} {
    unsafe {
        let pipe_in = os::pipe();
        let pipe_out = os::pipe();
        let pipe_err = os::pipe();
        let pid = spawn_process(prog, args, &None, &None,
                                pipe_in.in, pipe_out.out, pipe_err.out);

        os::close(pipe_in.in);
        os::close(pipe_out.out);
        os::close(pipe_err.out);
        if pid == -1i32 {
            os::close(pipe_in.out);
            os::close(pipe_out.in);
            os::close(pipe_err.in);
            fail;
        }

        os::close(pipe_in.out);

        // Spawn two entire schedulers to read both stdout and sterr
        // in parallel so we don't deadlock while blocking on one
        // or the other. FIXME (#2625): Surely there's a much more
        // clever way to do this.
        let p = oldcomm::Port();
        let ch = oldcomm::Chan(&p);
        do task::spawn_sched(task::SingleThreaded) {
            let errput = readclose(pipe_err.in);
            oldcomm::send(ch, (2, move errput));
        };
        do task::spawn_sched(task::SingleThreaded) {
            let output = readclose(pipe_out.in);
            oldcomm::send(ch, (1, move output));
        };
        let status = run::waitpid(pid);
        let mut errs = ~"";
        let mut outs = ~"";
        let mut count = 2;
        while count > 0 {
            let stream = oldcomm::recv(p);
            match stream {
                (1, copy s) => {
                    outs = move s;
                }
                (2, copy s) => {
                    errs = move s;
                }
                (n, _) => {
                    fail(fmt!("program_output received an unexpected file \
                               number: %u", n));
                }
            };
            count -= 1;
        };
        return {status: status, out: move outs, err: move errs};
    }
}

pub fn writeclose(fd: c_int, s: ~str) {
    use io::WriterUtil;

    error!("writeclose %d, %s", fd as int, s);
    let writer = io::fd_writer(fd, false);
    writer.write_str(s);

    os::close(fd);
}

pub fn readclose(fd: c_int) -> ~str {
    unsafe {
        let file = os::fdopen(fd);
        let reader = io::FILE_reader(file, false);
        let buf = io::with_bytes_writer(|writer| {
            let mut bytes = [mut 0, ..4096];
            while !reader.eof() {
                let nread = reader.read(bytes, bytes.len());
                writer.write(bytes.view(0, nread));
            }
        });
        os::fclose(file);
        str::from_bytes(buf)
    }
}

/// Waits for a process to exit and returns the exit code
pub fn waitpid(pid: pid_t) -> int {
    return waitpid_os(pid);

    #[cfg(windows)]
    fn waitpid_os(pid: pid_t) -> int {
        os::waitpid(pid) as int
    }

    #[cfg(unix)]
    fn waitpid_os(pid: pid_t) -> int {
        #[cfg(target_os = "linux")]
        #[cfg(target_os = "android")]
        fn WIFEXITED(status: i32) -> bool {
            (status & 0xffi32) == 0i32
        }

        #[cfg(target_os = "macos")]
        #[cfg(target_os = "freebsd")]
        fn WIFEXITED(status: i32) -> bool {
            (status & 0x7fi32) == 0i32
        }

        #[cfg(target_os = "linux")]
        #[cfg(target_os = "android")]
        fn WEXITSTATUS(status: i32) -> i32 {
            (status >> 8i32) & 0xffi32
        }

        #[cfg(target_os = "macos")]
        #[cfg(target_os = "freebsd")]
        fn WEXITSTATUS(status: i32) -> i32 {
            status >> 8i32
        }

        let status = os::waitpid(pid);
        return if WIFEXITED(status) {
            WEXITSTATUS(status) as int
        } else {
            1
        };
    }
}

#[cfg(test)]
mod tests {
    use debug;
    use io::WriterUtil;
    use option::{None, Some};
    use os;
    use run::{readclose, writeclose};
    use run;

    // Regression test for memory leaks
    #[ignore(cfg(windows))] // FIXME (#2626)
    pub fn test_leaks() {
        run::run_program("echo", []);
        run::start_program("echo", []);
        run::program_output("echo", []);
    }

    #[test]
    #[allow(non_implicitly_copyable_typarams)]
    pub fn test_pipes() {
        let pipe_in = os::pipe();
        let pipe_out = os::pipe();
        let pipe_err = os::pipe();

        let pid =
            run::spawn_process(
                "cat", [], &None, &None,
                pipe_in.in, pipe_out.out, pipe_err.out);
        os::close(pipe_in.in);
        os::close(pipe_out.out);
        os::close(pipe_err.out);

        if pid == -1i32 { fail; }
        let expected = ~"test";
        writeclose(pipe_in.out, copy expected);
        let actual = readclose(pipe_out.in);
        readclose(pipe_err.in);
        os::waitpid(pid);

        log(debug, copy expected);
        log(debug, copy actual);
        assert (expected == actual);
    }

    #[test]
    pub fn waitpid() {
        let pid = run::spawn_process("false", [],
                                     &None, &None,
                                     0i32, 0i32, 0i32);
        let status = run::waitpid(pid);
        assert status == 1;
    }

}

// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
