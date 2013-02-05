// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::prelude::*;

use io;
use io::WriterUtil;
use os;
use str;
use uint;
use vec;

use common;
use common::mode_run_pass;
use common::mode_run_fail;
use common::mode_compile_fail;
use common::mode_pretty;
use common::config;
use errors;
use header;
use header::load_props;
use header::TestProps;
use procsrv;
use util;
use util::logv;

pub fn run(config: config, testfile: ~str) {
    if config.verbose {
        // We're going to be dumping a lot of info. Start on a new line.
        io::stdout().write_str(~"\n\n");
    }
    let testfile = Path(testfile);
    debug!("running %s", testfile.to_str());
    let props = load_props(&testfile);
    match config.mode {
      mode_compile_fail => run_cfail_test(config, props, &testfile),
      mode_run_fail => run_rfail_test(config, props, &testfile),
      mode_run_pass => run_rpass_test(config, props, &testfile),
      mode_pretty => run_pretty_test(config, props, &testfile)
    }
}

fn run_cfail_test(config: config, props: TestProps, testfile: &Path) {
    let ProcRes = compile_test(config, props, testfile);

    if ProcRes.status == 0 {
        fatal_ProcRes(~"compile-fail test compiled successfully!", ProcRes);
    }

    check_correct_failure_status(ProcRes);

    let expected_errors = errors::load_errors(testfile);
    if !expected_errors.is_empty() {
        if !props.error_patterns.is_empty() {
            fatal(~"both error pattern and expected errors specified");
        }
        check_expected_errors(expected_errors, testfile, ProcRes);
    } else {
        check_error_patterns(props, testfile, ProcRes);
    }
}

fn run_rfail_test(config: config, props: TestProps, testfile: &Path) {
    let ProcRes = if !config.jit {
        let ProcRes = compile_test(config, props, testfile);

        if ProcRes.status != 0 {
            fatal_ProcRes(~"compilation failed!", ProcRes);
        }

        exec_compiled_test(config, props, testfile)
    } else {
        jit_test(config, props, testfile)
    };

    // The value our Makefile configures valgrind to return on failure
    const valgrind_err: int = 100;
    if ProcRes.status == valgrind_err {
        fatal_ProcRes(~"run-fail test isn't valgrind-clean!", ProcRes);
    }

    check_correct_failure_status(ProcRes);
    check_error_patterns(props, testfile, ProcRes);
}

fn check_correct_failure_status(ProcRes: ProcRes) {
    // The value the rust runtime returns on failure
    const rust_err: int = 101;
    if ProcRes.status != rust_err {
        fatal_ProcRes(
            fmt!("failure produced the wrong error code: %d",
                 ProcRes.status),
            ProcRes);
    }
}

fn run_rpass_test(config: config, props: TestProps, testfile: &Path) {
    if !config.jit {
        let mut ProcRes = compile_test(config, props, testfile);

        if ProcRes.status != 0 {
            fatal_ProcRes(~"compilation failed!", ProcRes);
        }

        ProcRes = exec_compiled_test(config, props, testfile);

        if ProcRes.status != 0 {
            fatal_ProcRes(~"test run failed!", ProcRes);
        }
    } else {
        let mut ProcRes = jit_test(config, props, testfile);

        if ProcRes.status != 0 { fatal_ProcRes(~"jit failed!", ProcRes); }
    }
}

fn run_pretty_test(config: config, props: TestProps, testfile: &Path) {
    if props.pp_exact.is_some() {
        logv(config, ~"testing for exact pretty-printing");
    } else { logv(config, ~"testing for converging pretty-printing"); }

    let rounds =
        match props.pp_exact { Some(_) => 1, None => 2 };

    let mut srcs = ~[io::read_whole_file_str(testfile).get()];

    let mut round = 0;
    while round < rounds {
        logv(config, fmt!("pretty-printing round %d", round));
        let ProcRes = print_source(config, testfile, srcs[round]);

        if ProcRes.status != 0 {
            fatal_ProcRes(fmt!("pretty-printing failed in round %d", round),
                          ProcRes);
        }

        srcs.push(ProcRes.stdout);
        round += 1;
    }

    let mut expected =
        match props.pp_exact {
          Some(file) => {
            let filepath = testfile.dir_path().push_rel(&file);
            io::read_whole_file_str(&filepath).get()
          }
          None => { srcs[vec::len(srcs) - 2u] }
        };
    let mut actual = srcs[vec::len(srcs) - 1u];

    if props.pp_exact.is_some() {
        // Now we have to care about line endings
        let cr = ~"\r";
        actual = str::replace(actual, cr, ~"");
        expected = str::replace(expected, cr, ~"");
    }

    compare_source(expected, actual);

    // Finally, let's make sure it actually appears to remain valid code
    let ProcRes = typecheck_source(config, props, testfile, actual);

    if ProcRes.status != 0 {
        fatal_ProcRes(~"pretty-printed source does not typecheck", ProcRes);
    }

    return;

    fn print_source(config: config, testfile: &Path, src: ~str) -> ProcRes {
        compose_and_run(config, testfile, make_pp_args(config, testfile),
                        ~[], config.compile_lib_path, Some(src))
    }

    fn make_pp_args(config: config, _testfile: &Path) -> ProcArgs {
        let prog = config.rustc_path;
        let args = ~[~"-", ~"--pretty", ~"normal"];
        return ProcArgs {prog: prog.to_str(), args: args};
    }

    fn compare_source(expected: ~str, actual: ~str) {
        if expected != actual {
            error(~"pretty-printed source does not match expected source");
            let msg =
                fmt!("\n\
expected:\n\
------------------------------------------\n\
%s\n\
------------------------------------------\n\
actual:\n\
------------------------------------------\n\
%s\n\
------------------------------------------\n\
\n",
                     expected, actual);
            io::stdout().write_str(msg);
            die!();
        }
    }

    fn typecheck_source(config: config, props: TestProps,
                        testfile: &Path, src: ~str) -> ProcRes {
        compose_and_run_compiler(
            config, props, testfile,
            make_typecheck_args(config, testfile),
            Some(src))
    }

    fn make_typecheck_args(config: config, testfile: &Path) -> ProcArgs {
        let prog = config.rustc_path;
        let mut args = ~[~"-",
                         ~"--no-trans", ~"--lib",
                         ~"-L", config.build_base.to_str(),
                         ~"-L",
                         aux_output_dir_name(config, testfile).to_str()];
        args += split_maybe_args(config.rustcflags);
        return ProcArgs {prog: prog.to_str(), args: args};
    }
}

fn check_error_patterns(props: TestProps,
                        testfile: &Path,
                        ProcRes: ProcRes) {
    if vec::is_empty(props.error_patterns) {
        fatal(~"no error pattern specified in " + testfile.to_str());
    }

    if ProcRes.status == 0 {
        fatal(~"process did not return an error status");
    }

    let mut next_err_idx = 0u;
    let mut next_err_pat = props.error_patterns[next_err_idx];
    let mut done = false;
    for str::split_char(ProcRes.stderr, '\n').each |line| {
        if str::contains(*line, next_err_pat) {
            debug!("found error pattern %s", next_err_pat);
            next_err_idx += 1u;
            if next_err_idx == vec::len(props.error_patterns) {
                debug!("found all error patterns");
                done = true;
                break;
            }
            next_err_pat = props.error_patterns[next_err_idx];
        }
    }
    if done { return; }

    let missing_patterns =
        vec::slice(props.error_patterns, next_err_idx,
                   vec::len(props.error_patterns));
    if vec::len(missing_patterns) == 1u {
        fatal_ProcRes(fmt!("error pattern '%s' not found!",
                           missing_patterns[0]), ProcRes);
    } else {
        for missing_patterns.each |pattern| {
            error(fmt!("error pattern '%s' not found!", *pattern));
        }
        fatal_ProcRes(~"multiple error patterns not found", ProcRes);
    }
}

fn check_expected_errors(expected_errors: ~[errors::ExpectedError],
                         testfile: &Path,
                         ProcRes: ProcRes) {

    // true if we found the error in question
    let found_flags = vec::cast_to_mut(vec::from_elem(
        vec::len(expected_errors), false));

    if ProcRes.status == 0 {
        fatal(~"process did not return an error status");
    }

    let prefixes = vec::map(expected_errors, |ee| {
        fmt!("%s:%u:", testfile.to_str(), ee.line)
    });

    // Scan and extract our error/warning messages,
    // which look like:
    //    filename:line1:col1: line2:col2: *error:* msg
    //    filename:line1:col1: line2:col2: *warning:* msg
    // where line1:col1: is the starting point, line2:col2:
    // is the ending point, and * represents ANSI color codes.
    for str::split_char(ProcRes.stderr, '\n').each |line| {
        let mut was_expected = false;
        for vec::eachi(expected_errors) |i, ee| {
            if !found_flags[i] {
                debug!("prefix=%s ee.kind=%s ee.msg=%s line=%s",
                       prefixes[i], ee.kind, ee.msg, *line);
                if (str::starts_with(*line, prefixes[i]) &&
                    str::contains(*line, ee.kind) &&
                    str::contains(*line, ee.msg)) {
                    found_flags[i] = true;
                    was_expected = true;
                    break;
                }
            }
        }

        // ignore this msg which gets printed at the end
        if str::contains(*line, ~"aborting due to") {
            was_expected = true;
        }

        if !was_expected && is_compiler_error_or_warning(*line) {
            fatal_ProcRes(fmt!("unexpected compiler error or warning: '%s'",
                               *line),
                          ProcRes);
        }
    }

    for uint::range(0u, vec::len(found_flags)) |i| {
        if !found_flags[i] {
            let ee = expected_errors[i];
            fatal_ProcRes(fmt!("expected %s on line %u not found: %s",
                               ee.kind, ee.line, ee.msg), ProcRes);
        }
    }
}

fn is_compiler_error_or_warning(line: ~str) -> bool {
    let mut i = 0u;
    return
        scan_until_char(line, ':', &mut i) &&
        scan_char(line, ':', &mut i) &&
        scan_integer(line, &mut i) &&
        scan_char(line, ':', &mut i) &&
        scan_integer(line, &mut i) &&
        scan_char(line, ':', &mut i) &&
        scan_char(line, ' ', &mut i) &&
        scan_integer(line, &mut i) &&
        scan_char(line, ':', &mut i) &&
        scan_integer(line, &mut i) &&
        scan_char(line, ' ', &mut i) &&
        (scan_string(line, ~"error", &mut i) ||
         scan_string(line, ~"warning", &mut i));
}

fn scan_until_char(haystack: ~str, needle: char, idx: &mut uint) -> bool {
    if *idx >= haystack.len() {
        return false;
    }
    let opt = str::find_char_from(haystack, needle, *idx);
    if opt.is_none() {
        return false;
    }
    *idx = opt.get();
    return true;
}

fn scan_char(haystack: ~str, needle: char, idx: &mut uint) -> bool {
    if *idx >= haystack.len() {
        return false;
    }
    let range = str::char_range_at(haystack, *idx);
    if range.ch != needle {
        return false;
    }
    *idx = range.next;
    return true;
}

fn scan_integer(haystack: ~str, idx: &mut uint) -> bool {
    let mut i = *idx;
    while i < haystack.len() {
        let range = str::char_range_at(haystack, i);
        if range.ch < '0' || '9' < range.ch {
            break;
        }
        i = range.next;
    }
    if i == *idx {
        return false;
    }
    *idx = i;
    return true;
}

fn scan_string(haystack: ~str, needle: ~str, idx: &mut uint) -> bool {
    let mut haystack_i = *idx;
    let mut needle_i = 0u;
    while needle_i < needle.len() {
        if haystack_i >= haystack.len() {
            return false;
        }
        let range = str::char_range_at(haystack, haystack_i);
        haystack_i = range.next;
        if !scan_char(needle, range.ch, &mut needle_i) {
            return false;
        }
    }
    *idx = haystack_i;
    return true;
}

struct ProcArgs {prog: ~str, args: ~[~str]}

struct ProcRes {status: int, stdout: ~str, stderr: ~str, cmdline: ~str}

fn compile_test(config: config, props: TestProps,
                testfile: &Path) -> ProcRes {
    compile_test_(config, props, testfile, [])
}

fn jit_test(config: config, props: TestProps, testfile: &Path) -> ProcRes {
    compile_test_(config, props, testfile, [~"--jit"])
}

fn compile_test_(config: config, props: TestProps,
                 testfile: &Path, extra_args: &[~str]) -> ProcRes {
    let link_args = ~[~"-L", aux_output_dir_name(config, testfile).to_str()];
    compose_and_run_compiler(
        config, props, testfile,
        make_compile_args(config, props, link_args + extra_args,
                          make_exe_name, testfile),
        None)
}

fn exec_compiled_test(config: config, props: TestProps,
                      testfile: &Path) -> ProcRes {
    compose_and_run(config, testfile,
                    make_run_args(config, props, testfile),
                    props.exec_env,
                    config.run_lib_path, None)
}

fn compose_and_run_compiler(
    config: config,
    props: TestProps,
    testfile: &Path,
    args: ProcArgs,
    input: Option<~str>) -> ProcRes {

    if !props.aux_builds.is_empty() {
        ensure_dir(&aux_output_dir_name(config, testfile));
    }

    let extra_link_args = ~[~"-L",
                            aux_output_dir_name(config, testfile).to_str()];

    for vec::each(props.aux_builds) |rel_ab| {
        let abs_ab = config.aux_base.push_rel(&Path(*rel_ab));
        let aux_args =
            make_compile_args(config, props, ~[~"--lib"] + extra_link_args,
                              |a,b| make_lib_name(a, b, testfile), &abs_ab);
        let auxres = compose_and_run(config, &abs_ab, aux_args, ~[],
                                     config.compile_lib_path, None);
        if auxres.status != 0 {
            fatal_ProcRes(
                fmt!("auxiliary build of %s failed to compile: ",
                     abs_ab.to_str()),
                auxres);
        }
    }

    compose_and_run(config, testfile, args, ~[],
                    config.compile_lib_path, input)
}

fn ensure_dir(path: &Path) {
    if os::path_is_dir(path) { return; }
    if !os::make_dir(path, 0x1c0i32) {
        die!(fmt!("can't make dir %s", path.to_str()));
    }
}

fn compose_and_run(config: config, testfile: &Path,
                   ProcArgs: ProcArgs,
                   procenv: ~[(~str, ~str)],
                   lib_path: ~str,
                   input: Option<~str>) -> ProcRes {
    return program_output(config, testfile, lib_path,
                       ProcArgs.prog, ProcArgs.args, procenv, input);
}

fn make_compile_args(config: config, props: TestProps, extras: ~[~str],
                     xform: fn(config, (&Path)) -> Path,
                     testfile: &Path) -> ProcArgs {
    let prog = config.rustc_path;
    let mut args = ~[testfile.to_str(),
                     ~"-o", xform(config, testfile).to_str(),
                     ~"-L", config.build_base.to_str()]
        + extras;
    args += split_maybe_args(config.rustcflags);
    args += split_maybe_args(props.compile_flags);
    return ProcArgs {prog: prog.to_str(), args: args};
}

fn make_lib_name(config: config, auxfile: &Path, testfile: &Path) -> Path {
    // what we return here is not particularly important, as it
    // happens; rustc ignores everything except for the directory.
    let auxname = output_testname(auxfile);
    aux_output_dir_name(config, testfile).push_rel(&auxname)
}

fn make_exe_name(config: config, testfile: &Path) -> Path {
    Path(output_base_name(config, testfile).to_str() +
            str::from_slice(os::EXE_SUFFIX))
}

fn make_run_args(config: config, _props: TestProps, testfile: &Path) ->
   ProcArgs {
    let toolargs = {
            // If we've got another tool to run under (valgrind),
            // then split apart its command
            let runtool =
                match config.runtool {
                  Some(s) => Some(s),
                  None => None
                };
            split_maybe_args(runtool)
        };

    let args = toolargs + ~[make_exe_name(config, testfile).to_str()];
    return ProcArgs {prog: args[0], args: vec::slice(args, 1, args.len())};
}

fn split_maybe_args(argstr: Option<~str>) -> ~[~str] {
    fn rm_whitespace(v: ~[~str]) -> ~[~str] {
        v.filtered(|s| !str::is_whitespace(*s))
    }

    match argstr {
      Some(s) => rm_whitespace(str::split_char(s, ' ')),
      None => ~[]
    }
}

fn program_output(config: config, testfile: &Path, lib_path: ~str, prog: ~str,
                  args: ~[~str], env: ~[(~str, ~str)],
                  input: Option<~str>) -> ProcRes {
    let cmdline =
        {
            let cmdline = make_cmdline(lib_path, prog, args);
            logv(config, fmt!("executing %s", cmdline));
            cmdline
        };
    let res = procsrv::run(lib_path, prog, args, env, input);
    dump_output(config, testfile, res.out, res.err);
    return ProcRes {status: res.status,
         stdout: res.out,
         stderr: res.err,
         cmdline: cmdline};
}

// Linux and mac don't require adjusting the library search path
#[cfg(target_os = "linux")]
#[cfg(target_os = "macos")]
#[cfg(target_os = "freebsd")]
fn make_cmdline(_libpath: ~str, prog: ~str, args: ~[~str]) -> ~str {
    fmt!("%s %s", prog, str::connect(args, ~" "))
}

#[cfg(target_os = "win32")]
fn make_cmdline(libpath: ~str, prog: ~str, args: ~[~str]) -> ~str {
    fmt!("%s %s %s", lib_path_cmd_prefix(libpath), prog,
         str::connect(args, ~" "))
}

// Build the LD_LIBRARY_PATH variable as it would be seen on the command line
// for diagnostic purposes
fn lib_path_cmd_prefix(path: ~str) -> ~str {
    fmt!("%s=\"%s\"", util::lib_path_env_var(), util::make_new_path(path))
}

fn dump_output(config: config, testfile: &Path, out: ~str, err: ~str) {
    dump_output_file(config, testfile, out, ~"out");
    dump_output_file(config, testfile, err, ~"err");
    maybe_dump_to_stdout(config, out, err);
}

fn dump_output_file(config: config, testfile: &Path,
                    out: ~str, extension: ~str) {
    let outfile = make_out_name(config, testfile, extension);
    let writer =
        io::file_writer(&outfile, ~[io::Create, io::Truncate]).get();
    writer.write_str(out);
}

fn make_out_name(config: config, testfile: &Path, extension: ~str) -> Path {
    output_base_name(config, testfile).with_filetype(extension)
}

fn aux_output_dir_name(config: config, testfile: &Path) -> Path {
    output_base_name(config, testfile).with_filetype("libaux")
}

fn output_testname(testfile: &Path) -> Path {
    Path(testfile.filestem().get())
}

fn output_base_name(config: config, testfile: &Path) -> Path {
    config.build_base
        .push_rel(&output_testname(testfile))
        .with_filetype(config.stage_id)
}

fn maybe_dump_to_stdout(config: config, out: ~str, err: ~str) {
    if config.verbose {
        let sep1 = fmt!("------%s------------------------------", ~"stdout");
        let sep2 = fmt!("------%s------------------------------", ~"stderr");
        let sep3 = ~"------------------------------------------";
        io::stdout().write_line(sep1);
        io::stdout().write_line(out);
        io::stdout().write_line(sep2);
        io::stdout().write_line(err);
        io::stdout().write_line(sep3);
    }
}

fn error(err: ~str) { io::stdout().write_line(fmt!("\nerror: %s", err)); }

fn fatal(err: ~str) -> ! { error(err); die!(); }

fn fatal_ProcRes(err: ~str, ProcRes: ProcRes) -> ! {
    let msg =
        fmt!("\n\
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
             err, ProcRes.cmdline, ProcRes.stdout, ProcRes.stderr);
    io::stdout().write_str(msg);
    die!();
}
