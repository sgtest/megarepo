// Copyright 2012-2013 The Rust Project Developers. See the
// COPYRIGHT file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use common::mode_run_pass;
use common::mode_run_fail;
use common::mode_compile_fail;
use common::mode_pretty;
use common::config;
use errors;
use header::load_props;
use header::TestProps;
use procsrv;
use util;
use util::logv;

use std::cell::Cell;
use std::rt::io;
use std::rt::io::Writer;
use std::rt::io::extensions::ReaderUtil;
use std::rt::io::file::FileInfo;
use std::os;
use std::str;
use std::task::{spawn_sched, SingleThreaded};
use std::vec;
use std::unstable::running_on_valgrind;

use extra::test::MetricMap;

pub fn run(config: config, testfile: ~str) {
    let config = Cell::new(config);
    let testfile = Cell::new(testfile);
    // FIXME #6436: Creating another thread to run the test because this
    // is going to call waitpid. The new scheduler has some strange
    // interaction between the blocking tasks and 'friend' schedulers
    // that destroys parallelism if we let normal schedulers block.
    // It should be possible to remove this spawn once std::run is
    // rewritten to be non-blocking.
    //
    // We do _not_ create another thread if we're running on V because
    // it serializes all threads anyways.
    if running_on_valgrind() {
        let config = config.take();
        let testfile = testfile.take();
        let mut _mm = MetricMap::new();
        run_metrics(config, testfile, &mut _mm);
    } else {
        do spawn_sched(SingleThreaded) {
            let config = config.take();
            let testfile = testfile.take();
            let mut _mm = MetricMap::new();
            run_metrics(config, testfile, &mut _mm);
        }
    }
}

pub fn run_metrics(config: config, testfile: ~str, mm: &mut MetricMap) {
    if config.verbose {
        // We're going to be dumping a lot of info. Start on a new line.
        print!("\n\n");
    }
    let testfile = Path::new(testfile);
    debug!("running {}", testfile.display());
    let props = load_props(&testfile);
    debug!("loaded props");
    match config.mode {
      mode_compile_fail => run_cfail_test(&config, &props, &testfile),
      mode_run_fail => run_rfail_test(&config, &props, &testfile),
      mode_run_pass => run_rpass_test(&config, &props, &testfile),
      mode_pretty => run_pretty_test(&config, &props, &testfile),
      mode_debug_info => run_debuginfo_test(&config, &props, &testfile),
      mode_codegen => run_codegen_test(&config, &props, &testfile, mm)
    }
}

fn run_cfail_test(config: &config, props: &TestProps, testfile: &Path) {
    let ProcRes = compile_test(config, props, testfile);

    if ProcRes.status == 0 {
        fatal_ProcRes(~"compile-fail test compiled successfully!", &ProcRes);
    }

    check_correct_failure_status(&ProcRes);

    let expected_errors = errors::load_errors(testfile);
    if !expected_errors.is_empty() {
        if !props.error_patterns.is_empty() {
            fatal(~"both error pattern and expected errors specified");
        }
        check_expected_errors(expected_errors, testfile, &ProcRes);
    } else {
        check_error_patterns(props, testfile, &ProcRes);
    }
}

fn run_rfail_test(config: &config, props: &TestProps, testfile: &Path) {
    let ProcRes = if !config.jit {
        let ProcRes = compile_test(config, props, testfile);

        if ProcRes.status != 0 {
            fatal_ProcRes(~"compilation failed!", &ProcRes);
        }

        exec_compiled_test(config, props, testfile)
    } else {
        jit_test(config, props, testfile)
    };

    // The value our Makefile configures valgrind to return on failure
    static VALGRIND_ERR: int = 100;
    if ProcRes.status == VALGRIND_ERR {
        fatal_ProcRes(~"run-fail test isn't valgrind-clean!", &ProcRes);
    }

    match config.target {

        ~"arm-linux-androideabi" => {
            if (config.adb_device_status) {
                check_correct_failure_status(&ProcRes);
                check_error_patterns(props, testfile, &ProcRes);
            }
        }

        _=> {
            check_correct_failure_status(&ProcRes);
            check_error_patterns(props, testfile, &ProcRes);
        }
    }
}

fn check_correct_failure_status(ProcRes: &ProcRes) {
    // The value the rust runtime returns on failure
    static RUST_ERR: int = 101;
    if ProcRes.status != RUST_ERR {
        fatal_ProcRes(
            format!("failure produced the wrong error code: {}",
                    ProcRes.status),
            ProcRes);
    }
}

fn run_rpass_test(config: &config, props: &TestProps, testfile: &Path) {
    if !config.jit {
        let mut ProcRes = compile_test(config, props, testfile);

        if ProcRes.status != 0 {
            fatal_ProcRes(~"compilation failed!", &ProcRes);
        }

        ProcRes = exec_compiled_test(config, props, testfile);

        if ProcRes.status != 0 {
            fatal_ProcRes(~"test run failed!", &ProcRes);
        }
    } else {
        let ProcRes = jit_test(config, props, testfile);

        if ProcRes.status != 0 { fatal_ProcRes(~"jit failed!", &ProcRes); }
    }
}

fn run_pretty_test(config: &config, props: &TestProps, testfile: &Path) {
    if props.pp_exact.is_some() {
        logv(config, ~"testing for exact pretty-printing");
    } else { logv(config, ~"testing for converging pretty-printing"); }

    let rounds =
        match props.pp_exact { Some(_) => 1, None => 2 };

    let src = testfile.open_reader(io::Open).read_to_end();
    let src = str::from_utf8_owned(src);
    let mut srcs = ~[src];

    let mut round = 0;
    while round < rounds {
        logv(config, format!("pretty-printing round {}", round));
        let ProcRes = print_source(config, testfile, srcs[round].clone());

        if ProcRes.status != 0 {
            fatal_ProcRes(format!("pretty-printing failed in round {}", round),
                          &ProcRes);
        }

        let ProcRes{ stdout, _ } = ProcRes;
        srcs.push(stdout);
        round += 1;
    }

    let mut expected = match props.pp_exact {
        Some(ref file) => {
            let filepath = testfile.dir_path().join(file);
            let s = filepath.open_reader(io::Open).read_to_end();
            str::from_utf8_owned(s)
          }
          None => { srcs[srcs.len() - 2u].clone() }
        };
    let mut actual = srcs[srcs.len() - 1u].clone();

    if props.pp_exact.is_some() {
        // Now we have to care about line endings
        let cr = ~"\r";
        actual = actual.replace(cr, "");
        expected = expected.replace(cr, "");
    }

    compare_source(expected, actual);

    // Finally, let's make sure it actually appears to remain valid code
    let ProcRes = typecheck_source(config, props, testfile, actual);

    if ProcRes.status != 0 {
        fatal_ProcRes(~"pretty-printed source does not typecheck", &ProcRes);
    }

    return;

    fn print_source(config: &config, testfile: &Path, src: ~str) -> ProcRes {
        compose_and_run(config, testfile, make_pp_args(config, testfile),
                        ~[], config.compile_lib_path, Some(src))
    }

    fn make_pp_args(config: &config, _testfile: &Path) -> ProcArgs {
        let args = ~[~"-", ~"--pretty", ~"normal"];
        // FIXME (#9639): This needs to handle non-utf8 paths
        return ProcArgs {prog: config.rustc_path.as_str().unwrap().to_owned(), args: args};
    }

    fn compare_source(expected: &str, actual: &str) {
        if expected != actual {
            error(~"pretty-printed source does not match expected source");
            println!("\n\
expected:\n\
------------------------------------------\n\
{}\n\
------------------------------------------\n\
actual:\n\
------------------------------------------\n\
{}\n\
------------------------------------------\n\
\n",
                     expected, actual);
            fail!();
        }
    }

    fn typecheck_source(config: &config, props: &TestProps,
                        testfile: &Path, src: ~str) -> ProcRes {
        let args = make_typecheck_args(config, props, testfile);
        compose_and_run_compiler(config, props, testfile, args, Some(src))
    }

    fn make_typecheck_args(config: &config, props: &TestProps, testfile: &Path) -> ProcArgs {
        let aux_dir = aux_output_dir_name(config, testfile);
        // FIXME (#9639): This needs to handle non-utf8 paths
        let mut args = ~[~"-",
                         ~"--no-trans", ~"--lib",
                         ~"-L", config.build_base.as_str().unwrap().to_owned(),
                         ~"-L",
                         aux_dir.as_str().unwrap().to_owned()];
        args.push_all_move(split_maybe_args(&config.rustcflags));
        args.push_all_move(split_maybe_args(&props.compile_flags));
        // FIXME (#9639): This needs to handle non-utf8 paths
        return ProcArgs {prog: config.rustc_path.as_str().unwrap().to_owned(), args: args};
    }
}

fn run_debuginfo_test(config: &config, props: &TestProps, testfile: &Path) {
    // do not optimize debuginfo tests
    let mut config = match config.rustcflags {
        Some(ref flags) => config {
            rustcflags: Some(flags.replace("-O", "")),
            .. (*config).clone()
        },
        None => (*config).clone()
    };
    let config = &mut config;
    let cmds = props.debugger_cmds.connect("\n");
    let check_lines = props.check_lines.clone();

    // compile test file (it shoud have 'compile-flags:-g' in the header)
    let mut ProcRes = compile_test(config, props, testfile);
    if ProcRes.status != 0 {
        fatal_ProcRes(~"compilation failed!", &ProcRes);
    }

    // write debugger script
    let script_str = [~"set charset UTF-8",
                      cmds,
                      ~"quit\n"].connect("\n");
    debug!("script_str = {}", script_str);
    dump_output_file(config, testfile, script_str, "debugger.script");

    // run debugger script with gdb
    #[cfg(windows)]
    fn debugger() -> ~str { ~"gdb.exe" }
    #[cfg(unix)]
    fn debugger() -> ~str { ~"gdb" }
    let debugger_script = make_out_name(config, testfile, "debugger.script");
    let exe_file = make_exe_name(config, testfile);
    // FIXME (#9639): This needs to handle non-utf8 paths
    let debugger_opts = ~[~"-quiet", ~"-batch", ~"-nx",
                          ~"-command=" + debugger_script.as_str().unwrap().to_owned(),
                          exe_file.as_str().unwrap().to_owned()];
    let ProcArgs = ProcArgs {prog: debugger(), args: debugger_opts};
    ProcRes = compose_and_run(config, testfile, ProcArgs, ~[], "", None);
    if ProcRes.status != 0 {
        fatal(~"gdb failed to execute");
    }

    let num_check_lines = check_lines.len();
    if num_check_lines > 0 {
        // check if each line in props.check_lines appears in the
        // output (in order)
        let mut i = 0u;
        for line in ProcRes.stdout.line_iter() {
            if check_lines[i].trim() == line.trim() {
                i += 1u;
            }
            if i == num_check_lines {
                // all lines checked
                break;
            }
        }
        if i != num_check_lines {
            fatal_ProcRes(format!("line not found in debugger output: {}",
                                  check_lines[i]), &ProcRes);
        }
    }
}

fn check_error_patterns(props: &TestProps,
                        testfile: &Path,
                        ProcRes: &ProcRes) {
    if props.error_patterns.is_empty() {
        do testfile.display().with_str |s| {
            fatal(~"no error pattern specified in " + s);
        }
    }

    if ProcRes.status == 0 {
        fatal(~"process did not return an error status");
    }

    let mut next_err_idx = 0u;
    let mut next_err_pat = &props.error_patterns[next_err_idx];
    let mut done = false;
    for line in ProcRes.stderr.line_iter() {
        if line.contains(*next_err_pat) {
            debug!("found error pattern {}", *next_err_pat);
            next_err_idx += 1u;
            if next_err_idx == props.error_patterns.len() {
                debug!("found all error patterns");
                done = true;
                break;
            }
            next_err_pat = &props.error_patterns[next_err_idx];
        }
    }
    if done { return; }

    let missing_patterns =
        props.error_patterns.slice(next_err_idx, props.error_patterns.len());
    if missing_patterns.len() == 1u {
        fatal_ProcRes(format!("error pattern '{}' not found!",
                              missing_patterns[0]), ProcRes);
    } else {
        for pattern in missing_patterns.iter() {
            error(format!("error pattern '{}' not found!", *pattern));
        }
        fatal_ProcRes(~"multiple error patterns not found", ProcRes);
    }
}

fn check_expected_errors(expected_errors: ~[errors::ExpectedError],
                         testfile: &Path,
                         ProcRes: &ProcRes) {

    // true if we found the error in question
    let mut found_flags = vec::from_elem(
        expected_errors.len(), false);

    if ProcRes.status == 0 {
        fatal(~"process did not return an error status");
    }

    let prefixes = expected_errors.iter().map(|ee| {
        format!("{}:{}:", testfile.display(), ee.line)
    }).collect::<~[~str]>();

    fn to_lower( s : &str ) -> ~str {
        let i = s.iter();
        let c : ~[char] = i.map( |c| {
            if c.is_ascii() {
                c.to_ascii().to_lower().to_char()
            } else {
                c
            }
        } ).collect();
        str::from_chars( c )
    }

    #[cfg(target_os = "win32")]
    fn prefix_matches( line : &str, prefix : &str ) -> bool {
        to_lower(line).starts_with( to_lower(prefix) )
    }

    #[cfg(target_os = "linux")]
    #[cfg(target_os = "macos")]
    #[cfg(target_os = "freebsd")]
    fn prefix_matches( line : &str, prefix : &str ) -> bool {
        line.starts_with( prefix )
    }

    // Scan and extract our error/warning messages,
    // which look like:
    //    filename:line1:col1: line2:col2: *error:* msg
    //    filename:line1:col1: line2:col2: *warning:* msg
    // where line1:col1: is the starting point, line2:col2:
    // is the ending point, and * represents ANSI color codes.
    for line in ProcRes.stderr.line_iter() {
        let mut was_expected = false;
        for (i, ee) in expected_errors.iter().enumerate() {
            if !found_flags[i] {
                debug!("prefix={} ee.kind={} ee.msg={} line={}",
                       prefixes[i], ee.kind, ee.msg, line);
                if (prefix_matches(line, prefixes[i]) &&
                    line.contains(ee.kind) &&
                    line.contains(ee.msg)) {
                    found_flags[i] = true;
                    was_expected = true;
                    break;
                }
            }
        }

        // ignore this msg which gets printed at the end
        if line.contains("aborting due to") {
            was_expected = true;
        }

        if !was_expected && is_compiler_error_or_warning(line) {
            fatal_ProcRes(format!("unexpected compiler error or warning: '{}'",
                               line),
                          ProcRes);
        }
    }

    for (i, &flag) in found_flags.iter().enumerate() {
        if !flag {
            let ee = &expected_errors[i];
            fatal_ProcRes(format!("expected {} on line {} not found: {}",
                               ee.kind, ee.line, ee.msg), ProcRes);
        }
    }
}

fn is_compiler_error_or_warning(line: &str) -> bool {
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
        (scan_string(line, "error", &mut i) ||
         scan_string(line, "warning", &mut i));
}

fn scan_until_char(haystack: &str, needle: char, idx: &mut uint) -> bool {
    if *idx >= haystack.len() {
        return false;
    }
    let opt = haystack.slice_from(*idx).find(needle);
    if opt.is_none() {
        return false;
    }
    *idx = opt.unwrap();
    return true;
}

fn scan_char(haystack: &str, needle: char, idx: &mut uint) -> bool {
    if *idx >= haystack.len() {
        return false;
    }
    let range = haystack.char_range_at(*idx);
    if range.ch != needle {
        return false;
    }
    *idx = range.next;
    return true;
}

fn scan_integer(haystack: &str, idx: &mut uint) -> bool {
    let mut i = *idx;
    while i < haystack.len() {
        let range = haystack.char_range_at(i);
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

fn scan_string(haystack: &str, needle: &str, idx: &mut uint) -> bool {
    let mut haystack_i = *idx;
    let mut needle_i = 0u;
    while needle_i < needle.len() {
        if haystack_i >= haystack.len() {
            return false;
        }
        let range = haystack.char_range_at(haystack_i);
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

fn compile_test(config: &config, props: &TestProps,
                testfile: &Path) -> ProcRes {
    compile_test_(config, props, testfile, [])
}

fn jit_test(config: &config, props: &TestProps, testfile: &Path) -> ProcRes {
    compile_test_(config, props, testfile, [~"--jit"])
}

fn compile_test_(config: &config, props: &TestProps,
                 testfile: &Path, extra_args: &[~str]) -> ProcRes {
    let aux_dir = aux_output_dir_name(config, testfile);
    // FIXME (#9639): This needs to handle non-utf8 paths
    let link_args = ~[~"-L", aux_dir.as_str().unwrap().to_owned()];
    let args = make_compile_args(config, props, link_args + extra_args,
                                 make_exe_name, testfile);
    compose_and_run_compiler(config, props, testfile, args, None)
}

fn exec_compiled_test(config: &config, props: &TestProps,
                      testfile: &Path) -> ProcRes {

    let env = props.exec_env.clone();

    match config.target {

        ~"arm-linux-androideabi" => {
            if (config.adb_device_status) {
                _arm_exec_compiled_test(config, props, testfile, env)
            } else {
                _dummy_exec_compiled_test(config, props, testfile)
            }
        }

        _=> {
            compose_and_run(config, testfile,
                            make_run_args(config, props, testfile),
                            env,
                            config.run_lib_path, None)
        }
    }
}

fn compose_and_run_compiler(
    config: &config,
    props: &TestProps,
    testfile: &Path,
    args: ProcArgs,
    input: Option<~str>) -> ProcRes {

    if !props.aux_builds.is_empty() {
        ensure_dir(&aux_output_dir_name(config, testfile));
    }

    let aux_dir = aux_output_dir_name(config, testfile);
    // FIXME (#9639): This needs to handle non-utf8 paths
    let extra_link_args = ~[~"-L", aux_dir.as_str().unwrap().to_owned()];

    for rel_ab in props.aux_builds.iter() {
        let abs_ab = config.aux_base.join(rel_ab.as_slice());
        let aux_args =
            make_compile_args(config, props, ~[~"--lib"] + extra_link_args,
                              |a,b| make_lib_name(a, b, testfile), &abs_ab);
        let auxres = compose_and_run(config, &abs_ab, aux_args, ~[],
                                     config.compile_lib_path, None);
        if auxres.status != 0 {
            fatal_ProcRes(
                format!("auxiliary build of {} failed to compile: ",
                     abs_ab.display()),
                &auxres);
        }

        match config.target {

            ~"arm-linux-androideabi" => {
                if (config.adb_device_status) {
                    _arm_push_aux_shared_library(config, testfile);
                }
            }

            _=> { }
        }
    }

    compose_and_run(config, testfile, args, ~[],
                    config.compile_lib_path, input)
}

fn ensure_dir(path: &Path) {
    if os::path_is_dir(path) { return; }
    if !os::make_dir(path, 0x1c0i32) {
        fail!("can't make dir {}", path.display());
    }
}

fn compose_and_run(config: &config, testfile: &Path,
                   ProcArgs{ args, prog }: ProcArgs,
                   procenv: ~[(~str, ~str)],
                   lib_path: &str,
                   input: Option<~str>) -> ProcRes {
    return program_output(config, testfile, lib_path,
                          prog, args, procenv, input);
}

fn make_compile_args(config: &config, props: &TestProps, extras: ~[~str],
                     xform: &fn(&config, (&Path)) -> Path,
                     testfile: &Path) -> ProcArgs {
    let xform_file = xform(config, testfile);
    // FIXME (#9639): This needs to handle non-utf8 paths
    let mut args = ~[testfile.as_str().unwrap().to_owned(),
                     ~"-o", xform_file.as_str().unwrap().to_owned(),
                     ~"-L", config.build_base.as_str().unwrap().to_owned()]
        + extras;
    args.push_all_move(split_maybe_args(&config.rustcflags));
    args.push_all_move(split_maybe_args(&props.compile_flags));
    return ProcArgs {prog: config.rustc_path.as_str().unwrap().to_owned(), args: args};
}

fn make_lib_name(config: &config, auxfile: &Path, testfile: &Path) -> Path {
    // what we return here is not particularly important, as it
    // happens; rustc ignores everything except for the directory.
    let auxname = output_testname(auxfile);
    aux_output_dir_name(config, testfile).join(&auxname)
}

fn make_exe_name(config: &config, testfile: &Path) -> Path {
    let mut f = output_base_name(config, testfile);
    if !os::EXE_SUFFIX.is_empty() {
        match f.filename().map(|s| s + os::EXE_SUFFIX.as_bytes()) {
            Some(v) => f.set_filename(v),
            None => ()
        }
    }
    f
}

fn make_run_args(config: &config, _props: &TestProps, testfile: &Path) ->
   ProcArgs {
    // If we've got another tool to run under (valgrind),
    // then split apart its command
    let mut args = split_maybe_args(&config.runtool);
    let exe_file = make_exe_name(config, testfile);
    // FIXME (#9639): This needs to handle non-utf8 paths
    args.push(exe_file.as_str().unwrap().to_owned());
    let prog = args.shift();
    return ProcArgs {prog: prog, args: args};
}

fn split_maybe_args(argstr: &Option<~str>) -> ~[~str] {
    match *argstr {
        Some(ref s) => {
            s.split_iter(' ')
                .filter_map(|s| if s.is_whitespace() {None} else {Some(s.to_owned())})
                .collect()
        }
        None => ~[]
    }
}

fn program_output(config: &config, testfile: &Path, lib_path: &str, prog: ~str,
                  args: ~[~str], env: ~[(~str, ~str)],
                  input: Option<~str>) -> ProcRes {
    let cmdline =
        {
            let cmdline = make_cmdline(lib_path, prog, args);
            logv(config, format!("executing {}", cmdline));
            cmdline
        };
    let procsrv::Result{ out, err, status } =
            procsrv::run(lib_path, prog, args, env, input);
    dump_output(config, testfile, out, err);
    return ProcRes {status: status,
         stdout: out,
         stderr: err,
         cmdline: cmdline};
}

// Linux and mac don't require adjusting the library search path
#[cfg(target_os = "linux")]
#[cfg(target_os = "macos")]
#[cfg(target_os = "freebsd")]
fn make_cmdline(_libpath: &str, prog: &str, args: &[~str]) -> ~str {
    format!("{} {}", prog, args.connect(" "))
}

#[cfg(target_os = "win32")]
fn make_cmdline(libpath: &str, prog: &str, args: &[~str]) -> ~str {
    format!("{} {} {}", lib_path_cmd_prefix(libpath), prog,
         args.connect(" "))
}

// Build the LD_LIBRARY_PATH variable as it would be seen on the command line
// for diagnostic purposes
fn lib_path_cmd_prefix(path: &str) -> ~str {
    format!("{}=\"{}\"", util::lib_path_env_var(), util::make_new_path(path))
}

fn dump_output(config: &config, testfile: &Path, out: &str, err: &str) {
    dump_output_file(config, testfile, out, "out");
    dump_output_file(config, testfile, err, "err");
    maybe_dump_to_stdout(config, out, err);
}

fn dump_output_file(config: &config, testfile: &Path,
                    out: &str, extension: &str) {
    let outfile = make_out_name(config, testfile, extension);
    outfile.open_writer(io::CreateOrTruncate).write(out.as_bytes());
}

fn make_out_name(config: &config, testfile: &Path, extension: &str) -> Path {
    output_base_name(config, testfile).with_extension(extension)
}

fn aux_output_dir_name(config: &config, testfile: &Path) -> Path {
    let mut f = output_base_name(config, testfile);
    match f.filename().map(|s| s + bytes!(".libaux")) {
        Some(v) => f.set_filename(v),
        None => ()
    }
    f
}

fn output_testname(testfile: &Path) -> Path {
    Path::new(testfile.filestem().unwrap())
}

fn output_base_name(config: &config, testfile: &Path) -> Path {
    config.build_base
        .join(&output_testname(testfile))
        .with_extension(config.stage_id.as_slice())
}

fn maybe_dump_to_stdout(config: &config, out: &str, err: &str) {
    if config.verbose {
        println!("------{}------------------------------", "stdout");
        println!("{}", out);
        println!("------{}------------------------------", "stderr");
        println!("{}", err);
        println!("------------------------------------------");
    }
}

fn error(err: ~str) { println!("\nerror: {}", err); }

fn fatal(err: ~str) -> ! { error(err); fail!(); }

fn fatal_ProcRes(err: ~str, ProcRes: &ProcRes) -> ! {
    print!("\n\
error: {}\n\
command: {}\n\
stdout:\n\
------------------------------------------\n\
{}\n\
------------------------------------------\n\
stderr:\n\
------------------------------------------\n\
{}\n\
------------------------------------------\n\
\n",
             err, ProcRes.cmdline, ProcRes.stdout, ProcRes.stderr);
    fail!();
}

fn _arm_exec_compiled_test(config: &config, props: &TestProps,
                      testfile: &Path, env: ~[(~str, ~str)]) -> ProcRes {

    let args = make_run_args(config, props, testfile);
    let cmdline = make_cmdline("", args.prog, args.args);

    // get bare program string
    let mut tvec: ~[~str] = args.prog.split_iter('/').map(|ts| ts.to_owned()).collect();
    let prog_short = tvec.pop();

    // copy to target
    let copy_result = procsrv::run("", config.adb_path,
        [~"push", args.prog.clone(), config.adb_test_dir.clone()],
        ~[(~"",~"")], Some(~""));

    if config.verbose {
        println!("push ({}) {} {} {}",
            config.target, args.prog,
            copy_result.out, copy_result.err);
    }

    logv(config, format!("executing ({}) {}", config.target, cmdline));

    let mut runargs = ~[];

    // run test via adb_run_wrapper
    runargs.push(~"shell");
    for (key, val) in env.move_iter() {
        runargs.push(format!("{}={}", key, val));
    }
    runargs.push(format!("{}/adb_run_wrapper.sh", config.adb_test_dir));
    runargs.push(format!("{}", config.adb_test_dir));
    runargs.push(format!("{}", prog_short));

    for tv in args.args.iter() {
        runargs.push(tv.to_owned());
    }

    procsrv::run("", config.adb_path, runargs, ~[(~"",~"")], Some(~""));

    // get exitcode of result
    runargs = ~[];
    runargs.push(~"shell");
    runargs.push(~"cat");
    runargs.push(format!("{}/{}.exitcode", config.adb_test_dir, prog_short));

    let procsrv::Result{ out: exitcode_out, err: _, status: _ } =
        procsrv::run("", config.adb_path, runargs, ~[(~"",~"")],
                     Some(~""));

    let mut exitcode : int = 0;
    for c in exitcode_out.iter() {
        if !c.is_digit() { break; }
        exitcode = exitcode * 10 + match c {
            '0' .. '9' => c as int - ('0' as int),
            _ => 101,
        }
    }

    // get stdout of result
    runargs = ~[];
    runargs.push(~"shell");
    runargs.push(~"cat");
    runargs.push(format!("{}/{}.stdout", config.adb_test_dir, prog_short));

    let procsrv::Result{ out: stdout_out, err: _, status: _ } =
        procsrv::run("", config.adb_path, runargs, ~[(~"",~"")], Some(~""));

    // get stderr of result
    runargs = ~[];
    runargs.push(~"shell");
    runargs.push(~"cat");
    runargs.push(format!("{}/{}.stderr", config.adb_test_dir, prog_short));

    let procsrv::Result{ out: stderr_out, err: _, status: _ } =
        procsrv::run("", config.adb_path, runargs, ~[(~"",~"")], Some(~""));

    dump_output(config, testfile, stdout_out, stderr_out);

    ProcRes {status: exitcode, stdout: stdout_out, stderr: stderr_out, cmdline: cmdline }
}

fn _dummy_exec_compiled_test(config: &config, props: &TestProps,
                      testfile: &Path) -> ProcRes {

    let args = make_run_args(config, props, testfile);
    let cmdline = make_cmdline("", args.prog, args.args);

    match config.mode {
        mode_run_fail => ProcRes {status: 101, stdout: ~"",
                                 stderr: ~"", cmdline: cmdline},
        _             => ProcRes {status: 0, stdout: ~"",
                                 stderr: ~"", cmdline: cmdline}
    }
}

fn _arm_push_aux_shared_library(config: &config, testfile: &Path) {
    let tdir = aux_output_dir_name(config, testfile);

    let dirs = os::list_dir_path(&tdir);
    for file in dirs.iter() {
        if file.extension_str() == Some("so") {
            // FIXME (#9639): This needs to handle non-utf8 paths
            let copy_result = procsrv::run("", config.adb_path,
                [~"push", file.as_str().unwrap().to_owned(), config.adb_test_dir.clone()],
                ~[(~"",~"")], Some(~""));

            if config.verbose {
                println!("push ({}) {} {} {}",
                    config.target, file.display(),
                    copy_result.out, copy_result.err);
            }
        }
    }
}

// codegen tests (vs. clang)

fn make_o_name(config: &config, testfile: &Path) -> Path {
    output_base_name(config, testfile).with_extension("o")
}

fn append_suffix_to_stem(p: &Path, suffix: &str) -> Path {
    if suffix.len() == 0 {
        (*p).clone()
    } else {
        let stem = p.filestem().unwrap();
        p.with_filename(stem + bytes!("-") + suffix.as_bytes())
    }
}

fn compile_test_and_save_bitcode(config: &config, props: &TestProps,
                                 testfile: &Path) -> ProcRes {
    let aux_dir = aux_output_dir_name(config, testfile);
    // FIXME (#9639): This needs to handle non-utf8 paths
    let link_args = ~[~"-L", aux_dir.as_str().unwrap().to_owned()];
    let llvm_args = ~[~"-c", ~"--lib", ~"--save-temps"];
    let args = make_compile_args(config, props,
                                 link_args + llvm_args,
                                 make_o_name, testfile);
    compose_and_run_compiler(config, props, testfile, args, None)
}

fn compile_cc_with_clang_and_save_bitcode(config: &config, _props: &TestProps,
                                          testfile: &Path) -> ProcRes {
    let bitcodefile = output_base_name(config, testfile).with_extension("bc");
    let bitcodefile = append_suffix_to_stem(&bitcodefile, "clang");
    let testcc = testfile.with_extension("cc");
    let ProcArgs = ProcArgs {
        // FIXME (#9639): This needs to handle non-utf8 paths
        prog: config.clang_path.get_ref().as_str().unwrap().to_owned(),
        args: ~[~"-c",
                ~"-emit-llvm",
                ~"-o", bitcodefile.as_str().unwrap().to_owned(),
                testcc.as_str().unwrap().to_owned() ]
    };
    compose_and_run(config, testfile, ProcArgs, ~[], "", None)
}

fn extract_function_from_bitcode(config: &config, _props: &TestProps,
                                 fname: &str, testfile: &Path,
                                 suffix: &str) -> ProcRes {
    let bitcodefile = output_base_name(config, testfile).with_extension("bc");
    let bitcodefile = append_suffix_to_stem(&bitcodefile, suffix);
    let extracted_bc = append_suffix_to_stem(&bitcodefile, "extract");
    let prog = config.llvm_bin_path.get_ref().join("llvm-extract");
    let ProcArgs = ProcArgs {
        // FIXME (#9639): This needs to handle non-utf8 paths
        prog: prog.as_str().unwrap().to_owned(),
        args: ~["-func=" + fname,
                "-o=" + extracted_bc.as_str().unwrap(),
                bitcodefile.as_str().unwrap().to_owned() ]
    };
    compose_and_run(config, testfile, ProcArgs, ~[], "", None)
}

fn disassemble_extract(config: &config, _props: &TestProps,
                       testfile: &Path, suffix: &str) -> ProcRes {
    let bitcodefile = output_base_name(config, testfile).with_extension("bc");
    let bitcodefile = append_suffix_to_stem(&bitcodefile, suffix);
    let extracted_bc = append_suffix_to_stem(&bitcodefile, "extract");
    let extracted_ll = extracted_bc.with_extension("ll");
    let prog = config.llvm_bin_path.get_ref().join("llvm-dis");
    let ProcArgs = ProcArgs {
        // FIXME (#9639): This needs to handle non-utf8 paths
        prog: prog.as_str().unwrap().to_owned(),
        args: ~["-o=" + extracted_ll.as_str().unwrap(),
                extracted_bc.as_str().unwrap().to_owned() ]
    };
    compose_and_run(config, testfile, ProcArgs, ~[], "", None)
}


fn count_extracted_lines(p: &Path) -> uint {
    let x = p.with_extension("ll").open_reader(io::Open).read_to_end();
    let x = str::from_utf8_owned(x);
    x.line_iter().len()
}


fn run_codegen_test(config: &config, props: &TestProps,
                    testfile: &Path, mm: &mut MetricMap) {

    if config.llvm_bin_path.is_none() {
        fatal(~"missing --llvm-bin-path");
    }

    if config.clang_path.is_none() {
        fatal(~"missing --clang-path");
    }

    let mut ProcRes = compile_test_and_save_bitcode(config, props, testfile);
    if ProcRes.status != 0 {
        fatal_ProcRes(~"compilation failed!", &ProcRes);
    }

    ProcRes = extract_function_from_bitcode(config, props, "test", testfile, "");
    if ProcRes.status != 0 {
        fatal_ProcRes(~"extracting 'test' function failed", &ProcRes);
    }

    ProcRes = disassemble_extract(config, props, testfile, "");
    if ProcRes.status != 0 {
        fatal_ProcRes(~"disassembling extract failed", &ProcRes);
    }


    let mut ProcRes = compile_cc_with_clang_and_save_bitcode(config, props, testfile);
    if ProcRes.status != 0 {
        fatal_ProcRes(~"compilation failed!", &ProcRes);
    }

    ProcRes = extract_function_from_bitcode(config, props, "test", testfile, "clang");
    if ProcRes.status != 0 {
        fatal_ProcRes(~"extracting 'test' function failed", &ProcRes);
    }

    ProcRes = disassemble_extract(config, props, testfile, "clang");
    if ProcRes.status != 0 {
        fatal_ProcRes(~"disassembling extract failed", &ProcRes);
    }

    let base = output_base_name(config, testfile);
    let base_extract = append_suffix_to_stem(&base, "extract");

    let base_clang = append_suffix_to_stem(&base, "clang");
    let base_clang_extract = append_suffix_to_stem(&base_clang, "extract");

    let base_lines = count_extracted_lines(&base_extract);
    let clang_lines = count_extracted_lines(&base_clang_extract);

    mm.insert_metric("clang-codegen-ratio",
                     (base_lines as f64) / (clang_lines as f64),
                     0.001);
}

