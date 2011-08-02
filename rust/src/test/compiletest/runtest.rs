import std::io;
import std::str;
import std::option;
import std::vec;
import std::fs;
import std::os;
import std::ivec;
import std::test;

import common::mode_run_pass;
import common::mode_run_fail;
import common::mode_compile_fail;
import common::cx;
import common::config;
import header::load_props;
import header::test_props;
import util::logv;

export run;

fn run(cx: &cx, testfile: &str) {
    test::configure_test_task();
    if (cx.config.verbose) {
        // We're going to be dumping a lot of info. Start on a new line.
        io::stdout().write_str("\n\n");
    }
    log #fmt("running %s", testfile);
    let props = load_props(testfile);
    alt cx.config.mode {
      mode_compile_fail. { run_cfail_test(cx, props, testfile); }
      mode_run_fail. { run_rfail_test(cx, props, testfile); }
      mode_run_pass. { run_rpass_test(cx, props, testfile); }
    }
}

fn run_cfail_test(cx: &cx, props: &test_props, testfile: &str) {
    let procres = compile_test(cx, props, testfile);

    if procres.status == 0 {
        fatal_procres("compile-fail test compiled successfully!",
                      procres);
    }

    check_error_patterns(props, testfile, procres);
}

fn run_rfail_test(cx: &cx, props: &test_props, testfile: &str) {
    let procres = compile_test(cx, props, testfile);

    if procres.status != 0 {
        fatal_procres("compilation failed!", procres); }

    procres = exec_compiled_test(cx, testfile);

    if procres.status == 0 {
        fatal_procres("run-fail test didn't produce an error!",
                      procres);
    }

    check_error_patterns(props, testfile, procres);
}

fn run_rpass_test(cx: &cx, props: &test_props, testfile: &str) {
    let procres = compile_test(cx, props, testfile);

    if procres.status != 0 {
        fatal_procres("compilation failed!", procres); }

    procres = exec_compiled_test(cx, testfile);


    if procres.status != 0 { fatal_procres("test run failed!", procres); }
}

fn check_error_patterns(props: &test_props, testfile: &str,
                        procres: &procres) {
    if ivec::is_empty(props.error_patterns) {
        fatal("no error pattern specified in " + testfile);
    }

    let next_err_idx = 0u;
    let next_err_pat = props.error_patterns.(next_err_idx);
    for line: str  in str::split(procres.stdout, '\n' as u8) {
        if str::find(line, next_err_pat) > 0 {
            log #fmt("found error pattern %s", next_err_pat);
            next_err_idx += 1u;
            if next_err_idx == ivec::len(props.error_patterns) {
                log "found all error patterns";
                ret;
            }
            next_err_pat = props.error_patterns.(next_err_idx);
        }
    }

    let missing_patterns =
        ivec::slice(props.error_patterns, next_err_idx,
                    ivec::len(props.error_patterns));
    if ivec::len(missing_patterns) == 1u {
        fatal_procres(#fmt("error pattern '%s' not found!",
                           missing_patterns.(0)), procres);
    } else {
        for pattern: str  in missing_patterns {
            error(#fmt("error pattern '%s' not found!", pattern));
        }
        fatal_procres("multiple error patterns not found", procres);
    }
}

type procargs = {prog: str, args: vec[str]};

type procres = {status: int, stdout: str, stderr: str, cmdline: str};

fn compile_test(cx: &cx, props: &test_props, testfile: &str) -> procres {
    compose_and_run(cx, testfile, bind make_compile_args(_, props, _),
                    cx.config.compile_lib_path)
}

fn exec_compiled_test(cx: &cx, testfile: &str) -> procres {
    compose_and_run(cx, testfile, make_run_args, cx.config.run_lib_path)
}

fn compose_and_run(cx: &cx, testfile: &str,
                   make_args: fn(&config, &str) -> procargs ,
                   lib_path: &str) -> procres {
    let procargs = make_args(cx.config, testfile);
    ret program_output(cx, testfile, lib_path,
                       procargs.prog, procargs.args);
}

fn make_compile_args(config: &config,
                     props: &test_props, testfile: &str) ->
    procargs {
    let prog = config.rustc_path;
    let args = [testfile, "-o", make_exe_name(config, testfile)];
    args += split_maybe_args(config.rustcflags);
    args += split_maybe_args(props.compile_flags);
    ret {prog: prog, args: args};
}

fn make_exe_name(config: &config, testfile: &str) -> str {
    output_base_name(config, testfile) + os::exec_suffix()
}

fn make_run_args(config: &config, testfile: &str) -> procargs {
    // If we've got another tool to run under (valgrind),
    // then split apart its command
    let args =
        split_maybe_args(config.runtool)
        + [make_exe_name(config, testfile)];
    ret {prog: args.(0), args: vec::slice(args, 1u, vec::len(args))};
}

fn split_maybe_args(argstr: &option::t[str]) -> vec[str] {
    fn rm_whitespace(v: vec[str]) -> vec[str] {
        fn flt(s: &str) -> option::t[str] {
            if !is_whitespace(s) {
                option::some(s)
            } else {
                option::none
            }
        }

        // FIXME: This should be in std
        fn is_whitespace(s: str) -> bool {
            for c: u8 in s {
                if c != (' ' as u8) { ret false; }
            }
            ret true;
        }
        vec::filter_map(flt, v)
    }

    alt argstr {
      option::some(s) { rm_whitespace(str::split(s, ' ' as u8)) }
      option::none. { [] }
    }
}

fn program_output(cx: &cx, testfile: &str, lib_path: &str, prog: &str,
                  args: &vec[str]) -> procres {
    let cmdline =
    {
        let cmdline = make_cmdline(lib_path, prog, args);
        logv(cx.config, #fmt("executing %s", cmdline));
        cmdline
    };
    let res = procsrv::run(cx.procsrv, lib_path, prog, args);
    dump_output(cx.config, testfile, res.out, res.err);
    ret {status: res.status, stdout: res.out,
         stderr: res.err, cmdline: cmdline};
}

fn make_cmdline(libpath: &str, prog: &str, args: &vec[str]) -> str {
    #fmt("%s %s %s", lib_path_cmd_prefix(libpath), prog,
         str::connect(args, " "))
}

// Build the LD_LIBRARY_PATH variable as it would be seen on the command line
// for diagnostic purposes
fn lib_path_cmd_prefix(path: &str) -> str {
    #fmt("%s=\"%s\"", util::lib_path_env_var(), util::make_new_path(path))
}

fn dump_output(config: &config, testfile: &str,
               out: &str, err: &str) {
    dump_output_file(config, testfile, out, "out");
    dump_output_file(config, testfile, err, "err");
    maybe_dump_to_stdout(config, out, err);
}

#[cfg(target_os = "win32")]
#[cfg(target_os = "linux")]
fn dump_output_file(config: &config, testfile: &str,
                    out: &str, extension: &str) {
    let outfile = make_out_name(config, testfile, extension);
    let writer = io::file_writer(outfile, [io::create, io::truncate]);
    writer.write_str(out);
}

// FIXME (726): Can't use file_writer on mac
#[cfg(target_os = "macos")]
fn dump_output_file(config: &config, testfile: &str,
                    out: &str, extension: &str) {
}

fn make_out_name(config: &config, testfile: &str,
                 extension: &str) -> str {
    output_base_name(config, testfile) + "." + extension
}

fn output_base_name(config: &config, testfile: &str) -> str {
    let base = config.build_base;
    let filename =
        {
            let parts = str::split(fs::basename(testfile), '.' as u8);
            parts = vec::slice(parts, 0u, vec::len(parts) - 1u);
            str::connect(parts, ".")
        };
    #fmt("%s%s.%s", base, filename, config.stage_id)
}

fn maybe_dump_to_stdout(config: &config,
                        out: &str, err: &str) {
    if config.verbose {
        let sep1 = #fmt("------%s------------------------------",
                        "stdout");
        let sep2 = #fmt("------%s------------------------------",
                        "stderr");
        let sep3 = "------------------------------------------";
        io::stdout().write_line(sep1);
        io::stdout().write_line(out);
        io::stdout().write_line(sep2);
        io::stdout().write_line(err);
        io::stdout().write_line(sep3);
    }
}

fn error(err: &str) { io::stdout().write_line(#fmt("\nerror: %s", err)); }

fn fatal(err: &str) -> ! { error(err); fail; }

fn fatal_procres(err: &str, procres: procres) -> ! {
    let msg =
        #fmt("\n\
error: %s\n\
command: %s\n\
stdout:\n\
------------------------------------------\n\
%s\n\
------------------------------------------\n\
stderr:\n\
------------------------------------------\n\
%s\n\
------------------------------------------\n\
\n",
             err, procres.cmdline, procres.stdout, procres.stderr);
    io::stdout().write_str(msg);
    fail;
}
