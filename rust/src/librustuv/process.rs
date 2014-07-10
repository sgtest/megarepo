// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use libc::c_int;
use libc;
use std::ptr;
use std::c_str::CString;
use std::rt::rtio;
use std::rt::rtio::IoResult;
use std::rt::task::BlockedTask;

use homing::{HomingIO, HomeHandle};
use pipe::PipeWatcher;
use super::{UvHandle, UvError, uv_error_to_io_error,
            wait_until_woken_after, wakeup, Loop};
use timer::TimerWatcher;
use uvio::UvIoFactory;
use uvll;

pub struct Process {
    handle: *mut uvll::uv_process_t,
    home: HomeHandle,

    /// Task to wake up (may be null) for when the process exits
    to_wake: Option<BlockedTask>,

    /// Collected from the exit_cb
    exit_status: Option<rtio::ProcessExit>,

    /// Lazily initialized timeout timer
    timer: Option<Box<TimerWatcher>>,
    timeout_state: TimeoutState,
}

enum TimeoutState {
    NoTimeout,
    TimeoutPending,
    TimeoutElapsed,
}

impl Process {
    /// Spawn a new process inside the specified event loop.
    ///
    /// Returns either the corresponding process object or an error which
    /// occurred.
    pub fn spawn(io_loop: &mut UvIoFactory, cfg: rtio::ProcessConfig)
                -> Result<(Box<Process>, Vec<Option<PipeWatcher>>), UvError> {
        let mut io = vec![cfg.stdin, cfg.stdout, cfg.stderr];
        for slot in cfg.extra_io.iter() {
            io.push(*slot);
        }
        let mut stdio = Vec::<uvll::uv_stdio_container_t>::with_capacity(io.len());
        let mut ret_io = Vec::with_capacity(io.len());
        unsafe {
            stdio.set_len(io.len());
            for (slot, other) in stdio.mut_iter().zip(io.iter()) {
                let io = set_stdio(slot as *mut uvll::uv_stdio_container_t, other,
                                   io_loop);
                ret_io.push(io);
            }
        }

        let ret = with_argv(cfg.program, cfg.args, |argv| {
            with_env(cfg.env, |envp| {
                let mut flags = 0;
                if cfg.uid.is_some() {
                    flags |= uvll::PROCESS_SETUID;
                }
                if cfg.gid.is_some() {
                    flags |= uvll::PROCESS_SETGID;
                }
                if cfg.detach {
                    flags |= uvll::PROCESS_DETACHED;
                }
                let mut options = uvll::uv_process_options_t {
                    exit_cb: on_exit,
                    file: unsafe { *argv },
                    args: argv,
                    env: envp,
                    cwd: match cfg.cwd {
                        Some(cwd) => cwd.as_ptr(),
                        None => ptr::null(),
                    },
                    flags: flags as libc::c_uint,
                    stdio_count: stdio.len() as libc::c_int,
                    stdio: stdio.as_mut_ptr(),
                    uid: cfg.uid.unwrap_or(0) as uvll::uv_uid_t,
                    gid: cfg.gid.unwrap_or(0) as uvll::uv_gid_t,
                };

                let handle = UvHandle::alloc(None::<Process>, uvll::UV_PROCESS);
                let process = box Process {
                    handle: handle,
                    home: io_loop.make_handle(),
                    to_wake: None,
                    exit_status: None,
                    timer: None,
                    timeout_state: NoTimeout,
                };
                match unsafe {
                    uvll::uv_spawn(io_loop.uv_loop(), handle, &mut options)
                } {
                    0 => Ok(process.install()),
                    err => Err(UvError(err)),
                }
            })
        });

        match ret {
            Ok(p) => Ok((p, ret_io)),
            Err(e) => Err(e),
        }
    }

    pub fn kill(pid: libc::pid_t, signum: int) -> Result<(), UvError> {
        match unsafe {
            uvll::uv_kill(pid as libc::c_int, signum as libc::c_int)
        } {
            0 => Ok(()),
            n => Err(UvError(n))
        }
    }
}

extern fn on_exit(handle: *mut uvll::uv_process_t,
                  exit_status: i64,
                  term_signal: libc::c_int) {
    let p: &mut Process = unsafe { UvHandle::from_uv_handle(&handle) };

    assert!(p.exit_status.is_none());
    p.exit_status = Some(match term_signal {
        0 => rtio::ExitStatus(exit_status as int),
        n => rtio::ExitSignal(n as int),
    });

    if p.to_wake.is_none() { return }
    wakeup(&mut p.to_wake);
}

unsafe fn set_stdio(dst: *mut uvll::uv_stdio_container_t,
                    io: &rtio::StdioContainer,
                    io_loop: &mut UvIoFactory) -> Option<PipeWatcher> {
    match *io {
        rtio::Ignored => {
            uvll::set_stdio_container_flags(dst, uvll::STDIO_IGNORE);
            None
        }
        rtio::InheritFd(fd) => {
            uvll::set_stdio_container_flags(dst, uvll::STDIO_INHERIT_FD);
            uvll::set_stdio_container_fd(dst, fd);
            None
        }
        rtio::CreatePipe(readable, writable) => {
            let mut flags = uvll::STDIO_CREATE_PIPE as libc::c_int;
            if readable {
                flags |= uvll::STDIO_READABLE_PIPE as libc::c_int;
            }
            if writable {
                flags |= uvll::STDIO_WRITABLE_PIPE as libc::c_int;
            }
            let pipe = PipeWatcher::new(io_loop, false);
            uvll::set_stdio_container_flags(dst, flags);
            uvll::set_stdio_container_stream(dst, pipe.handle());
            Some(pipe)
        }
    }
}

/// Converts the program and arguments to the argv array expected by libuv.
fn with_argv<T>(prog: &CString, args: &[CString],
                cb: |*const *const libc::c_char| -> T) -> T {
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

    cb(ptrs.as_ptr())
}

/// Converts the environment to the env array expected by libuv
fn with_env<T>(env: Option<&[(&CString, &CString)]>,
               cb: |*const *const libc::c_char| -> T) -> T {
    // We can pass a char** for envp, which is a null-terminated array
    // of "k=v\0" strings. Since we must create these strings locally,
    // yet expose a raw pointer to them, we create a temporary vector
    // to own the CStrings that outlives the call to cb.
    match env {
        Some(env) => {
            let mut tmps = Vec::with_capacity(env.len());

            for pair in env.iter() {
                let mut kv = Vec::new();
                kv.push_all(pair.ref0().as_bytes_no_nul());
                kv.push('=' as u8);
                kv.push_all(pair.ref1().as_bytes()); // includes terminal \0
                tmps.push(kv);
            }

            // As with `with_argv`, this is unsafe, since cb could leak the pointers.
            let mut ptrs: Vec<*const libc::c_char> =
                tmps.iter()
                    .map(|tmp| tmp.as_ptr() as *const libc::c_char)
                    .collect();
            ptrs.push(ptr::null());

            cb(ptrs.as_ptr())
        }
        _ => cb(ptr::null())
    }
}

impl HomingIO for Process {
    fn home<'r>(&'r mut self) -> &'r mut HomeHandle { &mut self.home }
}

impl UvHandle<uvll::uv_process_t> for Process {
    fn uv_handle(&self) -> *mut uvll::uv_process_t { self.handle }
}

impl rtio::RtioProcess for Process {
    fn id(&self) -> libc::pid_t {
        unsafe { uvll::process_pid(self.handle) as libc::pid_t }
    }

    fn kill(&mut self, signal: int) -> IoResult<()> {
        let _m = self.fire_homing_missile();
        match unsafe {
            uvll::uv_process_kill(self.handle, signal as libc::c_int)
        } {
            0 => Ok(()),
            err => Err(uv_error_to_io_error(UvError(err)))
        }
    }

    fn wait(&mut self) -> IoResult<rtio::ProcessExit> {
        // Make sure (on the home scheduler) that we have an exit status listed
        let _m = self.fire_homing_missile();
        match self.exit_status {
            Some(status) => return Ok(status),
            None => {}
        }

        // If there's no exit code previously listed, then the process's exit
        // callback has yet to be invoked. We just need to deschedule ourselves
        // and wait to be reawoken.
        match self.timeout_state {
            NoTimeout | TimeoutPending => {
                wait_until_woken_after(&mut self.to_wake, &self.uv_loop(), || {});
            }
            TimeoutElapsed => {}
        }

        // If there's still no exit status listed, then we timed out, and we
        // need to return.
        match self.exit_status {
            Some(status) => Ok(status),
            None => Err(uv_error_to_io_error(UvError(uvll::ECANCELED)))
        }
    }

    fn set_timeout(&mut self, timeout: Option<u64>) {
        let _m = self.fire_homing_missile();
        self.timeout_state = NoTimeout;
        let ms = match timeout {
            Some(ms) => ms,
            None => {
                match self.timer {
                    Some(ref mut timer) => timer.stop(),
                    None => {}
                }
                return
            }
        };
        if self.timer.is_none() {
            let loop_ = Loop::wrap(unsafe {
                uvll::get_loop_for_uv_handle(self.uv_handle())
            });
            let mut timer = box TimerWatcher::new_home(&loop_, self.home().clone());
            unsafe {
                timer.set_data(self as *mut _);
            }
            self.timer = Some(timer);
        }

        let timer = self.timer.get_mut_ref();
        timer.stop();
        timer.start(timer_cb, ms, 0);
        self.timeout_state = TimeoutPending;

        extern fn timer_cb(timer: *mut uvll::uv_timer_t) {
            let p: &mut Process = unsafe {
                &mut *(uvll::get_data_for_uv_handle(timer) as *mut Process)
            };
            p.timeout_state = TimeoutElapsed;
            match p.to_wake.take() {
                Some(task) => { let _t = task.wake().map(|t| t.reawaken()); }
                None => {}
            }
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _m = self.fire_homing_missile();
        assert!(self.to_wake.is_none());
        self.close();
    }
}
