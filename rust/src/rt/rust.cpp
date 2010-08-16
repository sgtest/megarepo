#include "rust_internal.h"


struct
command_line_args
{
    rust_dom &dom;
    int argc;
    char **argv;

    // vec[str] passed to rust_task::start.
    rust_vec *args;

    command_line_args(rust_dom &dom,
                      int sys_argc,
                      char **sys_argv)
        : dom(dom),
          argc(sys_argc),
          argv(sys_argv),
          args(NULL)
    {
#if defined(__WIN32__)
        LPCWSTR cmdline = GetCommandLineW();
        LPWSTR *wargv = CommandLineToArgvW(cmdline, &argc);
        dom.win32_require("CommandLineToArgvW", wargv != NULL);
        argv = (char **) dom.malloc(sizeof(char*) * argc);
        for (int i = 0; i < argc; ++i) {
            int n_chars = WideCharToMultiByte(CP_UTF8, 0, wargv[i], -1,
                                              NULL, 0, NULL, NULL);
            dom.win32_require("WideCharToMultiByte(0)", n_chars != 0);
            argv[i] = (char *) dom.malloc(n_chars);
            n_chars = WideCharToMultiByte(CP_UTF8, 0, wargv[i], -1,
                                          argv[i], n_chars, NULL, NULL);
            dom.win32_require("WideCharToMultiByte(1)", n_chars != 0);
        }
        LocalFree(wargv);
#endif
        size_t vec_fill = sizeof(rust_str *) * argc;
        size_t vec_alloc = next_power_of_two(sizeof(rust_vec) + vec_fill);
        void *mem = dom.malloc(vec_alloc);
        args = new (mem) rust_vec(&dom, vec_alloc, 0, NULL);
        rust_str **strs = (rust_str**) &args->data[0];
        for (int i = 0; i < argc; ++i) {
            size_t str_fill = strlen(argv[i]) + 1;
            size_t str_alloc = next_power_of_two(sizeof(rust_str) + str_fill);
            mem = dom.malloc(str_alloc);
            strs[i] = new (mem) rust_str(&dom, str_alloc, str_fill,
                                         (uint8_t const *)argv[i]);
        }
        args->fill = vec_fill;
        // If the caller has a declared args array, they may drop; but
        // we don't know if they have such an array. So we pin the args
        // array here to ensure it survives to program-shutdown.
        args->ref();
    }

    ~command_line_args() {
        if (args) {
            // Drop the args we've had pinned here.
            rust_str **strs = (rust_str**) &args->data[0];
            for (int i = 0; i < argc; ++i)
                dom.free(strs[i]);
            dom.free(args);
        }

#ifdef __WIN32__
        for (int i = 0; i < argc; ++i) {
            dom.free(argv[i]);
        }
        dom.free(argv);
#endif
    }
};


extern "C" CDECL int
rust_start(uintptr_t main_fn, rust_crate const *crate, int argc, char **argv)
{
    int ret;
    {
        rust_srv srv;
        rust_dom dom(&srv, crate, "main");
        command_line_args args(dom, argc, argv);

        dom.log(rust_log::DOM, "startup: %d args", args.argc);
        for (int i = 0; i < args.argc; ++i)
            dom.log(rust_log::DOM,
                    "startup: arg[%d] = '%s'", i, args.argv[i]);

        if (dom._log.is_tracing(rust_log::DWARF)) {
            rust_crate_reader rdr(&dom, crate);
        }

        uintptr_t main_args[4] = { 0, 0, 0, (uintptr_t)args.args };

        dom.root_task->start(crate->get_exit_task_glue(),
                             main_fn,
                             (uintptr_t)&main_args,
                             sizeof(main_args));

        ret = dom.start_main_loop();
    }

#if !defined(__WIN32__)
    // Don't take down the process if the main thread exits without an
    // error.
    if (!ret)
        pthread_exit(NULL);
#endif
    return ret;
}

//
// Local Variables:
// mode: C++
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C .. 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
//
