// So when running tests in parallel there's a potential race on environment
// variables if we let each task spawn its own children - between the time the
// environment is set and the process is spawned another task could spawn its
// child process. Because of that we have to use a complicated scheme with a
// dedicated server for spawning processes.

import std::option;
import std::task;
import std::task::task_id;
import std::generic_os::setenv;
import std::generic_os::getenv;
import std::vec;
import std::os;
import std::run;
import std::io;
import std::str;
import std::comm::_chan;
import std::comm::mk_port;
import std::comm::_port;
import std::comm::send;

export handle;
export mk;
export from_chan;
export run;
export close;
export reqchan;

type reqchan = _chan<request>;

type handle = {task: option::t<task_id>, chan: reqchan};

tag request {
    exec([u8], [u8], [[u8]], _chan<response>);
    stop;
}

type response = {pid: int, infd: int, outfd: int, errfd: int};

fn mk() -> handle {
    let setupport = mk_port();
    let task = task::_spawn(bind fn(setupchan: _chan<_chan<request>>) {
        let reqport = mk_port();
        let reqchan = reqport.mk_chan();
        send(setupchan, reqchan);
        worker(reqport);
    } (setupport.mk_chan()));
    ret {task: option::some(task),
         chan: setupport.recv()
        };
}

fn from_chan(ch: &reqchan) -> handle { {task: option::none, chan: ch} }

fn close(handle: &handle) {
    send(handle.chan, stop);
    task::join_id(option::get(handle.task));
}

fn run(handle: &handle, lib_path: &str,
       prog: &str, args: &[str], input: &option::t<str>) ->
{status: int, out: str, err: str} {
    let p = mk_port<response>();
    let ch = p.mk_chan();
    send(handle.chan, exec(str::bytes(lib_path),
                           str::bytes(prog),
                           clone_ivecstr(args),
                           ch));
    let resp = p.recv();

    writeclose(resp.infd, input);
    let output = readclose(resp.outfd);
    let errput = readclose(resp.errfd);
    let status = os::waitpid(resp.pid);
    ret {status: status, out: output, err: errput};
}

fn writeclose(fd: int, s: &option::t<str>) {
    if option::is_some(s) {
        let writer = io::new_writer(
            io::fd_buf_writer(fd, option::none));
        writer.write_str(option::get(s));
    }

    os::libc::close(fd);
}

fn readclose(fd: int) -> str {
    // Copied from run::program_output
    let file = os::fd_FILE(fd);
    let reader = io::new_reader(
        io::FILE_buf_reader(file, option::none));
    let buf = "";
    while !reader.eof() {
        let bytes = reader.read_bytes(4096u);
        buf += str::unsafe_from_bytes(bytes);
    }
    os::libc::fclose(file);
    ret buf;
}

fn worker(p: _port<request>) {

    // FIXME (787): If we declare this inside of the while loop and then
    // break out of it before it's ever initialized (i.e. we don't run
    // any tests), then the cleanups will puke, so we're initializing it
    // here with defaults.
    let execparms = {
        lib_path: "",
        prog: "",
        args: ~[],
        respchan: p.mk_chan()
    };

    while true {
        // FIXME: Sending strings across channels seems to still
        // leave them refed on the sender's end, which causes problems if
        // the receiver's poniters outlive the sender's. Here we clone
        // everything and let the originals go out of scope before sending
        // a response.
        execparms = {
            // FIXME (785): The 'discriminant' of an alt expression has
            // the same scope as the alt expression itself, so we have to
            // put the entire alt in another block to make sure the exec
            // message goes out of scope. Seems like the scoping rules for
            // the alt discriminant are wrong.
            alt p.recv() {
              exec(lib_path, prog, args, respchan) {
                {
                    lib_path: str::unsafe_from_bytes(lib_path),
                    prog: str::unsafe_from_bytes(prog),
                    args: clone_ivecu8str(args),
                    respchan: respchan
                }
              }
              stop. { ret }
            }
        };

        // This is copied from run::start_program
        let pipe_in = os::pipe();
        let pipe_out = os::pipe();
        let pipe_err = os::pipe();
        let spawnproc =
            bind run::spawn_process(execparms.prog,
                                    execparms.args,
                                    pipe_in.in,
                                    pipe_out.out,
                                    pipe_err.out);
        let pid = with_lib_path(execparms.lib_path, spawnproc);

        os::libc::close(pipe_in.in);
        os::libc::close(pipe_out.out);
        os::libc::close(pipe_err.out);
        if pid == -1 {
            os::libc::close(pipe_in.out);
            os::libc::close(pipe_out.in);
            os::libc::close(pipe_err.in);
            fail;
        }

        send(execparms.respchan,
             {pid: pid,
              infd: pipe_in.out,
              outfd: pipe_out.in,
              errfd: pipe_err.in});
    }
}

fn with_lib_path<T>(path: &str, f: fn() -> T ) -> T {
    let maybe_oldpath = getenv(util::lib_path_env_var());
    append_lib_path(path);
    let res = f();
    if option::is_some(maybe_oldpath) {
        export_lib_path(option::get(maybe_oldpath));
    } else {
        // FIXME: This should really be unset but we don't have that yet
        export_lib_path("");
    }
    ret res;
}

fn append_lib_path(path: &str) { export_lib_path(util::make_new_path(path)); }

fn export_lib_path(path: &str) { setenv(util::lib_path_env_var(), path); }

fn clone_ivecstr(v: &[str]) -> [[u8]] {
    let r = ~[];
    for t: str in vec::slice(v, 0u, vec::len(v)) {
        r += ~[str::bytes(t)];
    }
    ret r;
}

fn clone_ivecu8str(v: &[[u8]]) -> [str] {
    let r = ~[];
    for t in vec::slice(v, 0u, vec::len(v)) {
        r += ~[str::unsafe_from_bytes(t)];
    }
    ret r;
}
