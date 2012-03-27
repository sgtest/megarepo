#[doc(hidden)];

// Support code for rustc's built in test runner generator. Currently,
// none of this is meant for users. It is intended to support the
// simplest interface possible for representing and running tests
// while providing a base that other test frameworks may build off of.

import either::either;
import result::{ok, err};
import io::writer_util;

export test_name;
export test_fn;
export test_desc;
export test_main;
export test_result;
export test_opts;
export tr_ok;
export tr_failed;
export tr_ignored;
export run_tests_console;

#[abi = "cdecl"]
native mod rustrt {
    fn sched_threads() -> libc::size_t;
}

// The name of a test. By convention this follows the rules for rust
// paths; i.e. it should be a series of identifiers seperated by double
// colons. This way if some test runner wants to arrange the tests
// hierarchically it may.
type test_name = str;

// A function that runs a test. If the function returns successfully,
// the test succeeds; if the function fails then the test fails. We
// may need to come up with a more clever definition of test in order
// to support isolation of tests into tasks.
type test_fn = fn~();

// The definition of a single test. A test runner will run a list of
// these.
type test_desc = {
    name: test_name,
    fn: test_fn,
    ignore: bool,
    should_fail: bool
};

// The default console test runner. It accepts the command line
// arguments and a vector of test_descs (generated at compile time).
fn test_main(args: [str], tests: [test_desc]) {
    let opts =
        alt parse_opts(args) {
          either::left(o) { o }
          either::right(m) { fail m }
        };
    if !run_tests_console(opts, tests) { fail "Some tests failed"; }
}

type test_opts = {filter: option<str>, run_ignored: bool};

type opt_res = either<test_opts, str>;

// Parses command line arguments into test options
fn parse_opts(args: [str]) -> opt_res {
    let args_ = vec::tail(args);
    let opts = [getopts::optflag("ignored")];
    let match =
        alt getopts::getopts(args_, opts) {
          ok(m) { m }
          err(f) { ret either::right(getopts::fail_str(f)) }
        };

    let filter =
        if vec::len(match.free) > 0u {
            option::some(match.free[0])
        } else { option::none };

    let run_ignored = getopts::opt_present(match, "ignored");

    let test_opts = {filter: filter, run_ignored: run_ignored};

    ret either::left(test_opts);
}

enum test_result { tr_ok, tr_failed, tr_ignored, }

type console_test_state =
    @{out: io::writer,
      use_color: bool,
      mut total: uint,
      mut passed: uint,
      mut failed: uint,
      mut ignored: uint,
      mut failures: [test_desc]};

// A simple console test runner
fn run_tests_console(opts: test_opts,
                     tests: [test_desc]) -> bool {

    fn callback(event: testevent, st: console_test_state) {
        alt event {
          te_filtered(filtered_tests) {
            st.total = vec::len(filtered_tests);
            st.out.write_line(#fmt["\nrunning %u tests", st.total]);
          }
          te_wait(test) { st.out.write_str(#fmt["test %s ... ", test.name]); }
          te_result(test, result) {
            alt result {
              tr_ok {
                st.passed += 1u;
                write_ok(st.out, st.use_color);
                st.out.write_line("");
              }
              tr_failed {
                st.failed += 1u;
                write_failed(st.out, st.use_color);
                st.out.write_line("");
                st.failures += [test];
              }
              tr_ignored {
                st.ignored += 1u;
                write_ignored(st.out, st.use_color);
                st.out.write_line("");
              }
            }
          }
        }
    }

    let st =
        @{out: io::stdout(),
          use_color: use_color(),
          mut total: 0u,
          mut passed: 0u,
          mut failed: 0u,
          mut ignored: 0u,
          mut failures: []};

    run_tests(opts, tests, bind callback(_, st));

    assert (st.passed + st.failed + st.ignored == st.total);
    let success = st.failed == 0u;

    if !success {
        print_failures(st);
    }

    st.out.write_str(#fmt["\nresult: "]);
    if success {
        // There's no parallelism at this point so it's safe to use color
        write_ok(st.out, true);
    } else { write_failed(st.out, true); }
    st.out.write_str(#fmt[". %u passed; %u failed; %u ignored\n\n", st.passed,
                          st.failed, st.ignored]);

    ret success;

    fn write_ok(out: io::writer, use_color: bool) {
        write_pretty(out, "ok", term::color_green, use_color);
    }

    fn write_failed(out: io::writer, use_color: bool) {
        write_pretty(out, "FAILED", term::color_red, use_color);
    }

    fn write_ignored(out: io::writer, use_color: bool) {
        write_pretty(out, "ignored", term::color_yellow, use_color);
    }

    fn write_pretty(out: io::writer, word: str, color: u8, use_color: bool) {
        if use_color && term::color_supported() {
            term::fg(out, color);
        }
        out.write_str(word);
        if use_color && term::color_supported() {
            term::reset(out);
        }
    }
}

fn print_failures(st: console_test_state) {
    st.out.write_line("\nfailures:");
    let failures = vec::map(copy st.failures) {|test| test.name};
    let failures = sort::merge_sort(str::le, failures);
    for name in failures {
        st.out.write_line(#fmt["    %s", name]);
    }
}

#[test]
fn should_sort_failures_before_printing_them() {
    let buffer = io::mem_buffer();
    let writer = io::mem_buffer_writer(buffer);

    let test_a = {
        name: "a",
        fn: fn~() { },
        ignore: false,
        should_fail: false
    };

    let test_b = {
        name: "b",
        fn: fn~() { },
        ignore: false,
        should_fail: false
    };

    let st =
        @{out: writer,
          use_color: false,
          mut total: 0u,
          mut passed: 0u,
          mut failed: 0u,
          mut ignored: 0u,
          mut failures: [test_b, test_a]};

    print_failures(st);

    let s = io::mem_buffer_str(buffer);

    let apos = option::get(str::find_str(s, "a"));
    let bpos = option::get(str::find_str(s, "b"));
    assert apos < bpos;
}

fn use_color() -> bool { ret get_concurrency() == 1u; }

enum testevent {
    te_filtered([test_desc]),
    te_wait(test_desc),
    te_result(test_desc, test_result),
}

type monitor_msg = (test_desc, test_result);

fn run_tests(opts: test_opts, tests: [test_desc],
             callback: fn@(testevent)) {

    let mut filtered_tests = filter_tests(opts, tests);
    callback(te_filtered(filtered_tests));

    // It's tempting to just spawn all the tests at once, but since we have
    // many tests that run in other processes we would be making a big mess.
    let concurrency = get_concurrency();
    #debug("using %u test tasks", concurrency);

    let total = vec::len(filtered_tests);
    let mut run_idx = 0u;
    let mut wait_idx = 0u;
    let mut done_idx = 0u;

    let p = comm::port();
    let ch = comm::chan(p);

    while done_idx < total {
        while wait_idx < concurrency && run_idx < total {
            let test = vec::shift(filtered_tests);
            if concurrency == 1u {
                // We are doing one test at a time so we can print the name
                // of the test before we run it. Useful for debugging tests
                // that hang forever.
                callback(te_wait(test));
            }
            run_test(test, ch);
            wait_idx += 1u;
            run_idx += 1u;
        }

        let (test, result) = comm::recv(p);
        if concurrency != 1u {
            callback(te_wait(test));
        }
        callback(te_result(test, result));
        wait_idx -= 1u;
        done_idx += 1u;
    }
}

// Windows tends to dislike being overloaded with threads.
#[cfg(target_os = "win32")]
const sched_overcommit : uint = 1u;

#[cfg(target_os = "linux")]
#[cfg(target_os = "freebsd")]
#[cfg(target_os = "macos")]
const sched_overcommit : uint = 4u;

fn get_concurrency() -> uint {
    let threads = rustrt::sched_threads();
    if threads == 1u { 1u }
    else { threads * sched_overcommit }
}

fn filter_tests(opts: test_opts,
                tests: [test_desc]) -> [test_desc] {
    let mut filtered = tests;

    // Remove tests that don't match the test filter
    filtered = if option::is_none(opts.filter) {
        filtered
    } else {
        let filter_str =
            alt opts.filter {
          option::some(f) { f }
          option::none { "" }
        };

        fn filter_fn(test: test_desc, filter_str: str) ->
            option<test_desc> {
            if str::contains(test.name, filter_str) {
                ret option::some(test);
            } else { ret option::none; }
        }

        let filter = bind filter_fn(_, filter_str);

        vec::filter_map(filtered, filter)
    };

    // Maybe pull out the ignored test and unignore them
    filtered = if !opts.run_ignored {
        filtered
    } else {
        fn filter(test: test_desc) -> option<test_desc> {
            if test.ignore {
                ret option::some({name: test.name,
                                  fn: test.fn,
                                  ignore: false,
                                  should_fail: test.should_fail});
            } else { ret option::none; }
        };

        vec::filter_map(filtered, bind filter(_))
    };

    // Sort the tests alphabetically
    filtered =
        {
            fn lteq(t1: test_desc, t2: test_desc) -> bool {
                str::le(t1.name, t2.name)
            }
            sort::merge_sort(bind lteq(_, _), filtered)
        };

    ret filtered;
}

type test_future = {test: test_desc, wait: fn@() -> test_result};

fn run_test(+test: test_desc, monitor_ch: comm::chan<monitor_msg>) {
    if test.ignore {
        comm::send(monitor_ch, (test, tr_ignored));
        ret;
    }

    task::spawn {||
        let testfn = test.fn;
        let mut builder = task::task_builder();
        let result_future = task::future_result(builder);
        task::unsupervise(builder);
        task::run(builder, testfn);
        let task_result = future::get(result_future);
        let test_result = calc_result(test, task_result == task::success);
        comm::send(monitor_ch, (test, test_result));
    };
}

fn calc_result(test: test_desc, task_succeeded: bool) -> test_result {
    if task_succeeded {
        if test.should_fail { tr_failed }
        else { tr_ok }
    } else {
        if test.should_fail { tr_ok }
        else { tr_failed }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn do_not_run_ignored_tests() {
        fn f() { fail; }
        let desc = {
            name: "whatever",
            fn: f,
            ignore: true,
            should_fail: false
        };
        let p = comm::port();
        let ch = comm::chan(p);
        run_test(desc, ch);
        let (_, res) = comm::recv(p);
        assert res != tr_ok;
    }

    #[test]
    fn ignored_tests_result_in_ignored() {
        fn f() { }
        let desc = {
            name: "whatever",
            fn: f,
            ignore: true,
            should_fail: false
        };
        let p = comm::port();
        let ch = comm::chan(p);
        run_test(desc, ch);
        let (_, res) = comm::recv(p);
        assert res == tr_ignored;
    }

    #[test]
    #[ignore(cfg(target_os = "win32"))]
    fn test_should_fail() {
        fn f() { fail; }
        let desc = {
            name: "whatever",
            fn: f,
            ignore: false,
            should_fail: true
        };
        let p = comm::port();
        let ch = comm::chan(p);
        run_test(desc, ch);
        let (_, res) = comm::recv(p);
        assert res == tr_ok;
    }

    #[test]
    fn test_should_fail_but_succeeds() {
        fn f() { }
        let desc = {
            name: "whatever",
            fn: f,
            ignore: false,
            should_fail: true
        };
        let p = comm::port();
        let ch = comm::chan(p);
        run_test(desc, ch);
        let (_, res) = comm::recv(p);
        assert res == tr_failed;
    }

    #[test]
    fn first_free_arg_should_be_a_filter() {
        let args = ["progname", "filter"];
        let opts = alt parse_opts(args) { either::left(o) { o }
          _ { fail "Malformed arg in first_free_arg_should_be_a_filter"; } };
        assert (str::eq("filter", option::get(opts.filter)));
    }

    #[test]
    fn parse_ignored_flag() {
        let args = ["progname", "filter", "--ignored"];
        let opts = alt parse_opts(args) { either::left(o) { o }
          _ { fail "Malformed arg in parse_ignored_flag"; } };
        assert (opts.run_ignored);
    }

    #[test]
    fn filter_for_ignored_option() {
        // When we run ignored tests the test filter should filter out all the
        // unignored tests and flip the ignore flag on the rest to false

        let opts = {filter: option::none, run_ignored: true};
        let tests =
            [{name: "1", fn: fn~() { }, ignore: true, should_fail: false},
             {name: "2", fn: fn~() { }, ignore: false, should_fail: false}];
        let filtered = filter_tests(opts, tests);

        assert (vec::len(filtered) == 1u);
        assert (filtered[0].name == "1");
        assert (filtered[0].ignore == false);
    }

    #[test]
    fn sort_tests() {
        let opts = {filter: option::none, run_ignored: false};

        let names =
            ["sha1::test", "int::test_to_str", "int::test_pow",
             "test::do_not_run_ignored_tests",
             "test::ignored_tests_result_in_ignored",
             "test::first_free_arg_should_be_a_filter",
             "test::parse_ignored_flag", "test::filter_for_ignored_option",
             "test::sort_tests"];
        let tests =
        {
        let testfn = fn~() { };
        let mut tests = [];
        for name: str in names {
            let test = {name: name, fn: testfn, ignore: false,
                        should_fail: false};
            tests += [test];
        }
        tests
    };
    let filtered = filter_tests(opts, tests);

    let expected =
        ["int::test_pow", "int::test_to_str", "sha1::test",
         "test::do_not_run_ignored_tests", "test::filter_for_ignored_option",
         "test::first_free_arg_should_be_a_filter",
         "test::ignored_tests_result_in_ignored", "test::parse_ignored_flag",
         "test::sort_tests"];

    let pairs = vec::zip(expected, filtered);

    for (a, b) in pairs { assert (a == b.name); }
}
}


// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
