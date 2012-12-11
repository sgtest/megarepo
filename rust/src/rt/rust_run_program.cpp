// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


#include "rust_kernel.h"

#ifdef __APPLE__
#include <crt_externs.h>
#endif

#if defined(__WIN32__)

#include <process.h>
#include <io.h>

bool backslash_run_ends_in_quote(char const *c) {
    while (*c == '\\') ++c;
    return *c == '"';
}

void append_first_char(char *&buf, char const *c) {
    switch (*c) {

    case '"':
        // Escape quotes.
        *buf++ = '\\';
        *buf++ = '"';
        break;


    case '\\':
        if (backslash_run_ends_in_quote(c)) {
            // Double all backslashes that are in runs before quotes.
            *buf++ = '\\';
            *buf++ = '\\';
        } else {
            // Pass other backslashes through unescaped.
            *buf++ = '\\';
        }
        break;

    default:
        *buf++ = *c;
    }
}

bool contains_whitespace(char const *arg) {
    while (*arg) {
        switch (*arg++) {
        case ' ':
        case '\t':
            return true;
        }
    }
    return false;
}

void append_arg(char *& buf, char const *arg, bool last) {
    bool quote = contains_whitespace(arg);
    if (quote)
        *buf++ = '"';
    while (*arg)
        append_first_char(buf, arg++);
    if (quote)
        *buf++ = '"';

    if (! last) {
        *buf++ = ' ';
    } else {
        *buf++ = '\0';
    }
}

extern "C" CDECL int
rust_run_program(const char* argv[],
                 void* envp,
                 const char* dir,
                 int in_fd, int out_fd, int err_fd) {
    STARTUPINFO si;
    ZeroMemory(&si, sizeof(STARTUPINFO));
    si.cb = sizeof(STARTUPINFO);
    si.dwFlags = STARTF_USESTDHANDLES;

    HANDLE curproc = GetCurrentProcess();
    HANDLE origStdin = (HANDLE)_get_osfhandle(in_fd ? in_fd : 0);
    if (!DuplicateHandle(curproc, origStdin,
        curproc, &si.hStdInput, 0, 1, DUPLICATE_SAME_ACCESS))
        return -1;
    HANDLE origStdout = (HANDLE)_get_osfhandle(out_fd ? out_fd : 1);
    if (!DuplicateHandle(curproc, origStdout,
        curproc, &si.hStdOutput, 0, 1, DUPLICATE_SAME_ACCESS))
        return -1;
    HANDLE origStderr = (HANDLE)_get_osfhandle(err_fd ? err_fd : 2);
    if (!DuplicateHandle(curproc, origStderr,
        curproc, &si.hStdError, 0, 1, DUPLICATE_SAME_ACCESS))
        return -1;

    size_t cmd_len = 0;
    for (const char** arg = argv; *arg; arg++) {
        cmd_len += strlen(*arg);
        cmd_len += 3; // Two quotes plus trailing space or \0
    }
    cmd_len *= 2; // Potentially backslash-escape everything.

    char* cmd = (char*)malloc(cmd_len);
    char* pos = cmd;
    for (const char** arg = argv; *arg; arg++) {
        append_arg(pos, *arg, *(arg+1) == NULL);
    }

    PROCESS_INFORMATION pi;
    BOOL created = CreateProcess(NULL, cmd, NULL, NULL, TRUE,
                                 0, envp, dir, &si, &pi);

    CloseHandle(si.hStdInput);
    CloseHandle(si.hStdOutput);
    CloseHandle(si.hStdError);
    free(cmd);

    if (!created) return -1;
    return (int)pi.hProcess;
}

extern "C" CDECL int
rust_process_wait(int proc) {
    DWORD status;
    while (true) {
        if (GetExitCodeProcess((HANDLE)proc, &status) &&
            status != STILL_ACTIVE)
            return (int)status;
        WaitForSingleObject((HANDLE)proc, INFINITE);
    }
}

#elif defined(__GNUC__)

#include <sys/file.h>
#include <signal.h>
#include <sys/ioctl.h>
#include <unistd.h>
#include <termios.h>

#ifdef __FreeBSD__
extern char **environ;
#endif

extern "C" CDECL int
rust_run_program(const char* argv[],
                 void* envp,
                 const char* dir,
                 int in_fd, int out_fd, int err_fd) {
    int pid = fork();
    if (pid != 0) return pid;

    sigset_t sset;
    sigemptyset(&sset);
    sigprocmask(SIG_SETMASK, &sset, NULL);

    if (in_fd) dup2(in_fd, 0);
    if (out_fd) dup2(out_fd, 1);
    if (err_fd) dup2(err_fd, 2);
    /* Close all other fds. */
    for (int fd = getdtablesize() - 1; fd >= 3; fd--) close(fd);
    if (dir) {
        int result = chdir(dir);
        // FIXME (#2674): need error handling
        assert(!result && "chdir failed");
    }

    if (envp) {
#ifdef __APPLE__
        *_NSGetEnviron() = (char **)envp;
#else
        environ = (char **)envp;
#endif
    }

    execvp(argv[0], (char * const *)argv);
    exit(1);
}

extern "C" CDECL int
rust_process_wait(int proc) {
    // FIXME: stub; exists to placate linker. (#2692)
    return 0;
}

#else
#error "Platform not supported."
#endif

//
// Local Variables:
// mode: C++
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C $RBUILD 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
//
