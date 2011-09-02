
native "cdecl" mod libc = "" {
    fn read(fd: int, buf: *u8, count: uint) -> int;
    fn write(fd: int, buf: *u8, count: uint) -> int;
    fn fread(buf: *u8, size: uint, n: uint, f: libc::FILE) -> uint;
    fn fwrite(buf: *u8, size: uint, n: uint, f: libc::FILE) -> uint;
    fn open(s: str::sbuf, flags: int, mode: uint) -> int = "_open";
    fn close(fd: int) -> int = "_close";
    type FILE;
    fn fopen(path: str::sbuf, mode: str::sbuf) -> FILE;
    fn _fdopen(fd: int, mode: str::sbuf) -> FILE;
    fn fclose(f: FILE);
    fn fgetc(f: FILE) -> int;
    fn ungetc(c: int, f: FILE);
    fn feof(f: FILE) -> int;
    fn fseek(f: FILE, offset: int, whence: int) -> int;
    fn ftell(f: FILE) -> int;
    fn _pipe(fds: *mutable int, size: uint, mode: int) -> int;
}

mod libc_constants {
    fn O_RDONLY() -> int { ret 0; }
    fn O_WRONLY() -> int { ret 1; }
    fn O_RDWR() -> int { ret 2; }
    fn O_APPEND() -> int { ret 8; }
    fn O_CREAT() -> int { ret 256; }
    fn O_EXCL() -> int { ret 1024; }
    fn O_TRUNC() -> int { ret 512; }
    fn O_TEXT() -> int { ret 16384; }
    fn O_BINARY() -> int { ret 32768; }
    fn O_NOINHERIT() -> int { ret 128; }
    fn S_IRUSR() -> uint {
        ret 256u; // really _S_IREAD  in win32

    }
    fn S_IWUSR() -> uint {
        ret 128u; // really _S_IWRITE in win32

    }
}

native "x86stdcall" mod kernel32 {
    fn GetEnvironmentVariableA(n: str::sbuf, v: str::sbuf,
                               nsize: uint) -> uint;
    fn SetEnvironmentVariableA(n: str::sbuf, v: str::sbuf) -> int;
}

fn exec_suffix() -> istr { ret ~".exe"; }

fn target_os() -> istr { ret ~"win32"; }

fn dylib_filename(base: &istr) -> istr { ret base + ~".dll"; }

fn pipe() -> {in: int, out: int} {
    // Windows pipes work subtly differently than unix pipes, and their
    // inheritance has to be handled in a different way that I don't fully
    // understand. Here we explicitly make the pipe non-inheritable,
    // which means to pass it to a subprocess they need to be duplicated
    // first, as in rust_run_program.
    let fds = {mutable in: 0, mutable out: 0};
    let res =
        os::libc::_pipe(ptr::addr_of(fds.in), 1024u,
                        libc_constants::O_BINARY() |
                            libc_constants::O_NOINHERIT());
    assert (res == 0);
    assert (fds.in != -1 && fds.in != 0);
    assert (fds.out != -1 && fds.in != 0);
    ret {in: fds.in, out: fds.out};
}

fn fd_FILE(fd: int) -> libc::FILE {
    ret str::as_buf(~"r", { |modebuf|
        libc::_fdopen(fd, modebuf)
    });
}

native "rust" mod rustrt {
    fn rust_process_wait(handle: int) -> int;
    fn rust_getcwd() -> istr;
}

fn waitpid(pid: int) -> int { ret rustrt::rust_process_wait(pid); }

fn getcwd() -> istr {
    ret rustrt::rust_getcwd();
}

// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C $RBUILD 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
