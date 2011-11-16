
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
    const O_RDONLY: int    = 0;
    const O_WRONLY: int    = 1;
    const O_RDWR: int      = 2;
    const O_APPEND: int    = 8;
    const O_CREAT: int     = 256;
    const O_EXCL: int      = 1024;
    const O_TRUNC: int     = 512;
    const O_TEXT: int      = 16384;
    const O_BINARY: int    = 32768;
    const O_NOINHERIT: int = 128;
    const S_IRUSR: uint    = 256u; // really _S_IREAD  in win32
    const S_IWUSR: uint    = 128u; // really _S_IWRITE in win32
}

type DWORD = u32;
type HMODULE = uint;
type LPTSTR = str::sbuf;

native "stdcall" mod kernel32 {
    fn GetEnvironmentVariableA(n: str::sbuf, v: str::sbuf, nsize: uint) ->
       uint;
    fn SetEnvironmentVariableA(n: str::sbuf, v: str::sbuf) -> int;
    fn GetModuleFileNameA(hModule: HMODULE,
                          lpFilename: LPTSTR,
                          nSize: DWORD) -> DWORD;
}

// FIXME turn into constants
fn exec_suffix() -> str { ret ".exe"; }
fn target_os() -> str { ret "win32"; }

fn dylib_filename(base: str) -> str { ret base + ".dll"; }

fn pipe() -> {in: int, out: int} {
    // Windows pipes work subtly differently than unix pipes, and their
    // inheritance has to be handled in a different way that I don't fully
    // understand. Here we explicitly make the pipe non-inheritable,
    // which means to pass it to a subprocess they need to be duplicated
    // first, as in rust_run_program.
    let fds = {mutable in: 0, mutable out: 0};
    let res =
        os::libc::_pipe(ptr::mut_addr_of(fds.in), 1024u,
                        libc_constants::O_BINARY |
                            libc_constants::O_NOINHERIT);
    assert (res == 0);
    assert (fds.in != -1 && fds.in != 0);
    assert (fds.out != -1 && fds.in != 0);
    ret {in: fds.in, out: fds.out};
}

fn fd_FILE(fd: int) -> libc::FILE {
    ret str::as_buf("r", {|modebuf| libc::_fdopen(fd, modebuf) });
}

fn close(fd: int) -> int {
    libc::close(fd)
}

fn fclose(file: libc::FILE) {
    libc::fclose(file)
}

native "cdecl" mod rustrt {
    fn rust_process_wait(handle: int) -> int;
    fn rust_getcwd() -> str;
}

fn waitpid(pid: int) -> int { ret rustrt::rust_process_wait(pid); }

fn getcwd() -> str { ret rustrt::rust_getcwd(); }

fn get_exe_path() -> option::t<fs::path> {
    // FIXME: This doesn't handle the case where the buffer is too small
    let bufsize = 1023u;
    let path = str::unsafe_from_bytes(vec::init_elt(0u8, bufsize));
    ret str::as_buf(path, { |path_buf|
        if kernel32::GetModuleFileNameA(0u, path_buf,
                                        bufsize as u32) != 0u32 {
            option::some(fs::dirname(path) + fs::path_sep())
        } else {
            option::none
        }
    });
}

// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C $RBUILD 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
