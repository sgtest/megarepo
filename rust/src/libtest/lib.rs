// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Support code for rustc's built in unit-test and micro-benchmarking
//! framework.
//!
//! Almost all user code will only be interested in `Bencher` and
//! `black_box`. All other interactions (such as writing tests and
//! benchmarks themselves) should be done via the `#[test]` and
//! `#[bench]` attributes.
//!
//! See the [Testing Guide](../guide-testing.html) for more details.

// Currently, not much of this is meant for users. It is intended to
// support the simplest interface possible for representing and
// running tests while providing a base that other test frameworks may
// build off of.

#![crate_id = "test#0.11.0"] // NOTE: remove after stage0
#![crate_name = "test"] // NOTE: remove after stage0
#![experimental]
#![comment = "Rust internal test library only used by rustc"]
#![license = "MIT/ASL2"]
#![crate_type = "rlib"]
#![crate_type = "dylib"]
#![doc(html_logo_url = "http://www.rust-lang.org/logos/rust-logo-128x128-blk-v2.png",
       html_favicon_url = "http://www.rust-lang.org/favicon.ico",
       html_root_url = "http://doc.rust-lang.org/0.11.0/")]
#![allow(unused_attribute)] // NOTE: remove after stage0

#![feature(asm, macro_rules, phase)]

extern crate getopts;
extern crate regex;
extern crate serialize;
extern crate term;
extern crate time;

use std::collections::TreeMap;
use stats::Stats;
use time::precise_time_ns;
use getopts::{OptGroup, optflag, optopt};
use regex::Regex;
use serialize::{json, Decodable};
use serialize::json::{Json, ToJson};
use term::Terminal;
use term::color::{Color, RED, YELLOW, GREEN, CYAN};

use std::cmp;
use std::f64;
use std::fmt;
use std::fmt::Show;
use std::from_str::FromStr;
use std::io::stdio::StdWriter;
use std::io::{File, ChanReader, ChanWriter};
use std::io;
use std::os;
use std::str;
use std::string::String;
use std::task::TaskBuilder;

// to be used by rustc to compile tests in libtest
pub mod test {
    pub use {Bencher, TestName, TestResult, TestDesc,
             TestDescAndFn, TestOpts, TrFailed, TrIgnored, TrOk,
             Metric, MetricMap, MetricAdded, MetricRemoved,
             MetricChange, Improvement, Regression, LikelyNoise,
             StaticTestFn, StaticTestName, DynTestName, DynTestFn,
             run_test, test_main, test_main_static, filter_tests,
             parse_opts, StaticBenchFn};
}

pub mod stats;

// The name of a test. By convention this follows the rules for rust
// paths; i.e. it should be a series of identifiers separated by double
// colons. This way if some test runner wants to arrange the tests
// hierarchically it may.

#[deriving(Clone, PartialEq, Eq, Hash)]
pub enum TestName {
    StaticTestName(&'static str),
    DynTestName(String)
}
impl TestName {
    fn as_slice<'a>(&'a self) -> &'a str {
        match *self {
            StaticTestName(s) => s,
            DynTestName(ref s) => s.as_slice()
        }
    }
}
impl Show for TestName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_slice().fmt(f)
    }
}

#[deriving(Clone)]
enum NamePadding { PadNone, PadOnLeft, PadOnRight }

impl TestDesc {
    fn padded_name(&self, column_count: uint, align: NamePadding) -> String {
        use std::num::Saturating;
        let mut name = String::from_str(self.name.as_slice());
        let fill = column_count.saturating_sub(name.len());
        let mut pad = " ".repeat(fill);
        match align {
            PadNone => name,
            PadOnLeft => {
                pad.push_str(name.as_slice());
                pad
            }
            PadOnRight => {
                name.push_str(pad.as_slice());
                name
            }
        }
    }
}

/// Represents a benchmark function.
pub trait TDynBenchFn {
    fn run(&self, harness: &mut Bencher);
}

// A function that runs a test. If the function returns successfully,
// the test succeeds; if the function fails then the test fails. We
// may need to come up with a more clever definition of test in order
// to support isolation of tests into tasks.
pub enum TestFn {
    StaticTestFn(fn()),
    StaticBenchFn(fn(&mut Bencher)),
    StaticMetricFn(proc(&mut MetricMap)),
    DynTestFn(proc():Send),
    DynMetricFn(proc(&mut MetricMap)),
    DynBenchFn(Box<TDynBenchFn>)
}

impl TestFn {
    fn padding(&self) -> NamePadding {
        match self {
            &StaticTestFn(..)   => PadNone,
            &StaticBenchFn(..)  => PadOnRight,
            &StaticMetricFn(..) => PadOnRight,
            &DynTestFn(..)      => PadNone,
            &DynMetricFn(..)    => PadOnRight,
            &DynBenchFn(..)     => PadOnRight,
        }
    }
}

impl fmt::Show for TestFn {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write(match *self {
            StaticTestFn(..) => "StaticTestFn(..)",
            StaticBenchFn(..) => "StaticBenchFn(..)",
            StaticMetricFn(..) => "StaticMetricFn(..)",
            DynTestFn(..) => "DynTestFn(..)",
            DynMetricFn(..) => "DynMetricFn(..)",
            DynBenchFn(..) => "DynBenchFn(..)"
        }.as_bytes())
    }
}

/// Manager of the benchmarking runs.
///
/// This is feed into functions marked with `#[bench]` to allow for
/// set-up & tear-down before running a piece of code repeatedly via a
/// call to `iter`.
pub struct Bencher {
    iterations: u64,
    ns_start: u64,
    ns_end: u64,
    pub bytes: u64,
}

// The definition of a single test. A test runner will run a list of
// these.
#[deriving(Clone, Show, PartialEq, Eq, Hash)]
pub struct TestDesc {
    pub name: TestName,
    pub ignore: bool,
    pub should_fail: bool,
}

#[deriving(Show)]
pub struct TestDescAndFn {
    pub desc: TestDesc,
    pub testfn: TestFn,
}

#[deriving(Clone, Encodable, Decodable, PartialEq, Show)]
pub struct Metric {
    value: f64,
    noise: f64
}

impl Metric {
    pub fn new(value: f64, noise: f64) -> Metric {
        Metric {value: value, noise: noise}
    }
}

#[deriving(PartialEq)]
pub struct MetricMap(TreeMap<String,Metric>);

impl Clone for MetricMap {
    fn clone(&self) -> MetricMap {
        let MetricMap(ref map) = *self;
        MetricMap(map.clone())
    }
}

/// Analysis of a single change in metric
#[deriving(PartialEq, Show)]
pub enum MetricChange {
    LikelyNoise,
    MetricAdded,
    MetricRemoved,
    Improvement(f64),
    Regression(f64)
}

pub type MetricDiff = TreeMap<String,MetricChange>;

// The default console test runner. It accepts the command line
// arguments and a vector of test_descs.
pub fn test_main(args: &[String], tests: Vec<TestDescAndFn> ) {
    let opts =
        match parse_opts(args) {
            Some(Ok(o)) => o,
            Some(Err(msg)) => fail!("{}", msg),
            None => return
        };
    match run_tests_console(&opts, tests) {
        Ok(true) => {}
        Ok(false) => fail!("Some tests failed"),
        Err(e) => fail!("io error when running tests: {}", e),
    }
}

// A variant optimized for invocation with a static test vector.
// This will fail (intentionally) when fed any dynamic tests, because
// it is copying the static values out into a dynamic vector and cannot
// copy dynamic values. It is doing this because from this point on
// a ~[TestDescAndFn] is used in order to effect ownership-transfer
// semantics into parallel test runners, which in turn requires a ~[]
// rather than a &[].
pub fn test_main_static(args: &[String], tests: &[TestDescAndFn]) {
    let owned_tests = tests.iter().map(|t| {
        match t.testfn {
            StaticTestFn(f) => TestDescAndFn { testfn: StaticTestFn(f), desc: t.desc.clone() },
            StaticBenchFn(f) => TestDescAndFn { testfn: StaticBenchFn(f), desc: t.desc.clone() },
            _ => fail!("non-static tests passed to test::test_main_static")
        }
    }).collect();
    test_main(args, owned_tests)
}

pub enum ColorConfig {
    AutoColor,
    AlwaysColor,
    NeverColor,
}

pub struct TestOpts {
    pub filter: Option<Regex>,
    pub run_ignored: bool,
    pub run_tests: bool,
    pub run_benchmarks: bool,
    pub ratchet_metrics: Option<Path>,
    pub ratchet_noise_percent: Option<f64>,
    pub save_metrics: Option<Path>,
    pub test_shard: Option<(uint,uint)>,
    pub logfile: Option<Path>,
    pub nocapture: bool,
    pub color: ColorConfig,
}

impl TestOpts {
    #[cfg(test)]
    fn new() -> TestOpts {
        TestOpts {
            filter: None,
            run_ignored: false,
            run_tests: false,
            run_benchmarks: false,
            ratchet_metrics: None,
            ratchet_noise_percent: None,
            save_metrics: None,
            test_shard: None,
            logfile: None,
            nocapture: false,
            color: AutoColor,
        }
    }
}

/// Result of parsing the options.
pub type OptRes = Result<TestOpts, String>;

fn optgroups() -> Vec<getopts::OptGroup> {
    vec!(getopts::optflag("", "ignored", "Run ignored tests"),
      getopts::optflag("", "test", "Run tests and not benchmarks"),
      getopts::optflag("", "bench", "Run benchmarks instead of tests"),
      getopts::optflag("h", "help", "Display this message (longer with --help)"),
      getopts::optopt("", "save-metrics", "Location to save bench metrics",
                     "PATH"),
      getopts::optopt("", "ratchet-metrics",
                     "Location to load and save metrics from. The metrics \
                      loaded are cause benchmarks to fail if they run too \
                      slowly", "PATH"),
      getopts::optopt("", "ratchet-noise-percent",
                     "Tests within N% of the recorded metrics will be \
                      considered as passing", "PERCENTAGE"),
      getopts::optopt("", "logfile", "Write logs to the specified file instead \
                          of stdout", "PATH"),
      getopts::optopt("", "test-shard", "run shard A, of B shards, worth of the testsuite",
                     "A.B"),
      getopts::optflag("", "nocapture", "don't capture stdout/stderr of each \
                                         task, allow printing directly"),
      getopts::optopt("", "color", "Configure coloring of output:
            auto   = colorize if stdout is a tty and tests are run on serially (default);
            always = always colorize output;
            never  = never colorize output;", "auto|always|never"))
}

fn usage(binary: &str) {
    let message = format!("Usage: {} [OPTIONS] [FILTER]", binary);
    println!(r"{usage}

The FILTER regex is tested against the name of all tests to run, and
only those tests that match are run.

By default, all tests are run in parallel. This can be altered with the
RUST_TEST_TASKS environment variable when running tests (set it to 1).

All tests have their standard output and standard error captured by default.
This can be overridden with the --nocapture flag or the RUST_TEST_NOCAPTURE=1
environment variable. Logging is not captured by default.

Test Attributes:

    #[test]        - Indicates a function is a test to be run. This function
                     takes no arguments.
    #[bench]       - Indicates a function is a benchmark to be run. This
                     function takes one argument (test::Bencher).
    #[should_fail] - This function (also labeled with #[test]) will only pass if
                     the code causes a failure (an assertion failure or fail!)
    #[ignore]      - When applied to a function which is already attributed as a
                     test, then the test runner will ignore these tests during
                     normal test runs. Running with --ignored will run these
                     tests. This may also be written as #[ignore(cfg(...))] to
                     ignore the test on certain configurations.",
             usage = getopts::usage(message.as_slice(),
                                    optgroups().as_slice()));
}

// Parses command line arguments into test options
pub fn parse_opts(args: &[String]) -> Option<OptRes> {
    let args_ = args.tail();
    let matches =
        match getopts::getopts(args_.as_slice(), optgroups().as_slice()) {
          Ok(m) => m,
          Err(f) => return Some(Err(f.to_str()))
        };

    if matches.opt_present("h") { usage(args[0].as_slice()); return None; }

    let filter = if matches.free.len() > 0 {
        let s = matches.free.get(0).as_slice();
        match Regex::new(s) {
            Ok(re) => Some(re),
            Err(e) => return Some(Err(format!("could not parse /{}/: {}", s, e)))
        }
    } else {
        None
    };

    let run_ignored = matches.opt_present("ignored");

    let logfile = matches.opt_str("logfile");
    let logfile = logfile.map(|s| Path::new(s));

    let run_benchmarks = matches.opt_present("bench");
    let run_tests = ! run_benchmarks ||
        matches.opt_present("test");

    let ratchet_metrics = matches.opt_str("ratchet-metrics");
    let ratchet_metrics = ratchet_metrics.map(|s| Path::new(s));

    let ratchet_noise_percent = matches.opt_str("ratchet-noise-percent");
    let ratchet_noise_percent =
        ratchet_noise_percent.map(|s| from_str::<f64>(s.as_slice()).unwrap());

    let save_metrics = matches.opt_str("save-metrics");
    let save_metrics = save_metrics.map(|s| Path::new(s));

    let test_shard = matches.opt_str("test-shard");
    let test_shard = opt_shard(test_shard);

    let mut nocapture = matches.opt_present("nocapture");
    if !nocapture {
        nocapture = os::getenv("RUST_TEST_NOCAPTURE").is_some();
    }

    let color = match matches.opt_str("color").as_ref().map(|s| s.as_slice()) {
        Some("auto") | None => AutoColor,
        Some("always") => AlwaysColor,
        Some("never") => NeverColor,

        Some(v) => return Some(Err(format!("argument for --color must be \
                                            auto, always, or never (was {})",
                                            v))),
    };

    let test_opts = TestOpts {
        filter: filter,
        run_ignored: run_ignored,
        run_tests: run_tests,
        run_benchmarks: run_benchmarks,
        ratchet_metrics: ratchet_metrics,
        ratchet_noise_percent: ratchet_noise_percent,
        save_metrics: save_metrics,
        test_shard: test_shard,
        logfile: logfile,
        nocapture: nocapture,
        color: color,
    };

    Some(Ok(test_opts))
}

pub fn opt_shard(maybestr: Option<String>) -> Option<(uint,uint)> {
    match maybestr {
        None => None,
        Some(s) => {
            let mut it = s.as_slice().split('.');
            match (it.next().and_then(from_str::<uint>), it.next().and_then(from_str::<uint>),
                   it.next()) {
                (Some(a), Some(b), None) => {
                    if a <= 0 || a > b {
                        fail!("tried to run shard {a}.{b}, but {a} is out of bounds \
                              (should be between 1 and {b}", a=a, b=b)
                    }
                    Some((a, b))
                }
                _ => None,
            }
        }
    }
}


#[deriving(Clone, PartialEq)]
pub struct BenchSamples {
    ns_iter_summ: stats::Summary<f64>,
    mb_s: uint,
}

#[deriving(Clone, PartialEq)]
pub enum TestResult {
    TrOk,
    TrFailed,
    TrIgnored,
    TrMetrics(MetricMap),
    TrBench(BenchSamples),
}

enum OutputLocation<T> {
    Pretty(Box<term::Terminal<term::WriterWrapper> + Send>),
    Raw(T),
}

struct ConsoleTestState<T> {
    log_out: Option<File>,
    out: OutputLocation<T>,
    use_color: bool,
    total: uint,
    passed: uint,
    failed: uint,
    ignored: uint,
    measured: uint,
    metrics: MetricMap,
    failures: Vec<(TestDesc, Vec<u8> )> ,
    max_name_len: uint, // number of columns to fill when aligning names
}

impl<T: Writer> ConsoleTestState<T> {
    pub fn new(opts: &TestOpts,
               _: Option<T>) -> io::IoResult<ConsoleTestState<StdWriter>> {
        let log_out = match opts.logfile {
            Some(ref path) => Some(try!(File::create(path))),
            None => None
        };
        let out = match term::stdout() {
            None => Raw(io::stdio::stdout_raw()),
            Some(t) => Pretty(t)
        };

        Ok(ConsoleTestState {
            out: out,
            log_out: log_out,
            use_color: use_color(opts),
            total: 0u,
            passed: 0u,
            failed: 0u,
            ignored: 0u,
            measured: 0u,
            metrics: MetricMap::new(),
            failures: Vec::new(),
            max_name_len: 0u,
        })
    }

    pub fn write_ok(&mut self) -> io::IoResult<()> {
        self.write_pretty("ok", term::color::GREEN)
    }

    pub fn write_failed(&mut self) -> io::IoResult<()> {
        self.write_pretty("FAILED", term::color::RED)
    }

    pub fn write_ignored(&mut self) -> io::IoResult<()> {
        self.write_pretty("ignored", term::color::YELLOW)
    }

    pub fn write_metric(&mut self) -> io::IoResult<()> {
        self.write_pretty("metric", term::color::CYAN)
    }

    pub fn write_bench(&mut self) -> io::IoResult<()> {
        self.write_pretty("bench", term::color::CYAN)
    }

    pub fn write_added(&mut self) -> io::IoResult<()> {
        self.write_pretty("added", term::color::GREEN)
    }

    pub fn write_improved(&mut self) -> io::IoResult<()> {
        self.write_pretty("improved", term::color::GREEN)
    }

    pub fn write_removed(&mut self) -> io::IoResult<()> {
        self.write_pretty("removed", term::color::YELLOW)
    }

    pub fn write_regressed(&mut self) -> io::IoResult<()> {
        self.write_pretty("regressed", term::color::RED)
    }

    pub fn write_pretty(&mut self,
                        word: &str,
                        color: term::color::Color) -> io::IoResult<()> {
        match self.out {
            Pretty(ref mut term) => {
                if self.use_color {
                    try!(term.fg(color));
                }
                try!(term.write(word.as_bytes()));
                if self.use_color {
                    try!(term.reset());
                }
                Ok(())
            }
            Raw(ref mut stdout) => stdout.write(word.as_bytes())
        }
    }

    pub fn write_plain(&mut self, s: &str) -> io::IoResult<()> {
        match self.out {
            Pretty(ref mut term) => term.write(s.as_bytes()),
            Raw(ref mut stdout) => stdout.write(s.as_bytes())
        }
    }

    pub fn write_run_start(&mut self, len: uint) -> io::IoResult<()> {
        self.total = len;
        let noun = if len != 1 { "tests" } else { "test" };
        self.write_plain(format!("\nrunning {} {}\n", len, noun).as_slice())
    }

    pub fn write_test_start(&mut self, test: &TestDesc,
                            align: NamePadding) -> io::IoResult<()> {
        let name = test.padded_name(self.max_name_len, align);
        self.write_plain(format!("test {} ... ", name).as_slice())
    }

    pub fn write_result(&mut self, result: &TestResult) -> io::IoResult<()> {
        try!(match *result {
            TrOk => self.write_ok(),
            TrFailed => self.write_failed(),
            TrIgnored => self.write_ignored(),
            TrMetrics(ref mm) => {
                try!(self.write_metric());
                self.write_plain(format!(": {}", fmt_metrics(mm)).as_slice())
            }
            TrBench(ref bs) => {
                try!(self.write_bench());
                self.write_plain(format!(": {}",
                                         fmt_bench_samples(bs)).as_slice())
            }
        });
        self.write_plain("\n")
    }

    pub fn write_log(&mut self, test: &TestDesc,
                     result: &TestResult) -> io::IoResult<()> {
        match self.log_out {
            None => Ok(()),
            Some(ref mut o) => {
                let s = format!("{} {}\n", match *result {
                        TrOk => "ok".to_string(),
                        TrFailed => "failed".to_string(),
                        TrIgnored => "ignored".to_string(),
                        TrMetrics(ref mm) => fmt_metrics(mm),
                        TrBench(ref bs) => fmt_bench_samples(bs)
                    }, test.name.as_slice());
                o.write(s.as_bytes())
            }
        }
    }

    pub fn write_failures(&mut self) -> io::IoResult<()> {
        try!(self.write_plain("\nfailures:\n"));
        let mut failures = Vec::new();
        let mut fail_out = String::new();
        for &(ref f, ref stdout) in self.failures.iter() {
            failures.push(f.name.to_str());
            if stdout.len() > 0 {
                fail_out.push_str(format!("---- {} stdout ----\n\t",
                                          f.name.as_slice()).as_slice());
                let output = str::from_utf8_lossy(stdout.as_slice());
                fail_out.push_str(output.as_slice()
                                        .replace("\n", "\n\t")
                                        .as_slice());
                fail_out.push_str("\n");
            }
        }
        if fail_out.len() > 0 {
            try!(self.write_plain("\n"));
            try!(self.write_plain(fail_out.as_slice()));
        }

        try!(self.write_plain("\nfailures:\n"));
        failures.as_mut_slice().sort();
        for name in failures.iter() {
            try!(self.write_plain(format!("    {}\n",
                                          name.as_slice()).as_slice()));
        }
        Ok(())
    }

    pub fn write_metric_diff(&mut self, diff: &MetricDiff) -> io::IoResult<()> {
        let mut noise = 0u;
        let mut improved = 0u;
        let mut regressed = 0u;
        let mut added = 0u;
        let mut removed = 0u;

        for (k, v) in diff.iter() {
            match *v {
                LikelyNoise => noise += 1,
                MetricAdded => {
                    added += 1;
                    try!(self.write_added());
                    try!(self.write_plain(format!(": {}\n", *k).as_slice()));
                }
                MetricRemoved => {
                    removed += 1;
                    try!(self.write_removed());
                    try!(self.write_plain(format!(": {}\n", *k).as_slice()));
                }
                Improvement(pct) => {
                    improved += 1;
                    try!(self.write_plain(format!(": {}", *k).as_slice()));
                    try!(self.write_improved());
                    try!(self.write_plain(format!(" by {:.2f}%\n",
                                                  pct as f64).as_slice()));
                }
                Regression(pct) => {
                    regressed += 1;
                    try!(self.write_plain(format!(": {}", *k).as_slice()));
                    try!(self.write_regressed());
                    try!(self.write_plain(format!(" by {:.2f}%\n",
                                                  pct as f64).as_slice()));
                }
            }
        }
        try!(self.write_plain(format!("result of ratchet: {} metrics added, \
                                        {} removed, {} improved, {} regressed, \
                                        {} noise\n",
                                       added, removed, improved, regressed,
                                       noise).as_slice()));
        if regressed == 0 {
            try!(self.write_plain("updated ratchet file\n"));
        } else {
            try!(self.write_plain("left ratchet file untouched\n"));
        }
        Ok(())
    }

    pub fn write_run_finish(&mut self,
                            ratchet_metrics: &Option<Path>,
                            ratchet_pct: Option<f64>) -> io::IoResult<bool> {
        assert!(self.passed + self.failed + self.ignored + self.measured == self.total);

        let ratchet_success = match *ratchet_metrics {
            None => true,
            Some(ref pth) => {
                try!(self.write_plain(format!("\nusing metrics ratchet: {}\n",
                                              pth.display()).as_slice()));
                match ratchet_pct {
                    None => (),
                    Some(pct) =>
                        try!(self.write_plain(format!("with noise-tolerance \
                                                         forced to: {}%\n",
                                                        pct).as_slice()))
                }
                let (diff, ok) = self.metrics.ratchet(pth, ratchet_pct);
                try!(self.write_metric_diff(&diff));
                ok
            }
        };

        let test_success = self.failed == 0u;
        if !test_success {
            try!(self.write_failures());
        }

        let success = ratchet_success && test_success;

        try!(self.write_plain("\ntest result: "));
        if success {
            // There's no parallelism at this point so it's safe to use color
            try!(self.write_ok());
        } else {
            try!(self.write_failed());
        }
        let s = format!(". {} passed; {} failed; {} ignored; {} measured\n\n",
                        self.passed, self.failed, self.ignored, self.measured);
        try!(self.write_plain(s.as_slice()));
        return Ok(success);
    }
}

pub fn fmt_metrics(mm: &MetricMap) -> String {
    let MetricMap(ref mm) = *mm;
    let v : Vec<String> = mm.iter()
        .map(|(k,v)| format!("{}: {} (+/- {})", *k,
                             v.value as f64, v.noise as f64))
        .collect();
    v.connect(", ")
}

pub fn fmt_bench_samples(bs: &BenchSamples) -> String {
    if bs.mb_s != 0 {
        format!("{:>9} ns/iter (+/- {}) = {} MB/s",
             bs.ns_iter_summ.median as uint,
             (bs.ns_iter_summ.max - bs.ns_iter_summ.min) as uint,
             bs.mb_s)
    } else {
        format!("{:>9} ns/iter (+/- {})",
             bs.ns_iter_summ.median as uint,
             (bs.ns_iter_summ.max - bs.ns_iter_summ.min) as uint)
    }
}

// A simple console test runner
pub fn run_tests_console(opts: &TestOpts, tests: Vec<TestDescAndFn> ) -> io::IoResult<bool> {

    fn callback<T: Writer>(event: &TestEvent, st: &mut ConsoleTestState<T>) -> io::IoResult<()> {
        match (*event).clone() {
            TeFiltered(ref filtered_tests) => st.write_run_start(filtered_tests.len()),
            TeWait(ref test, padding) => st.write_test_start(test, padding),
            TeResult(test, result, stdout) => {
                try!(st.write_log(&test, &result));
                try!(st.write_result(&result));
                match result {
                    TrOk => st.passed += 1,
                    TrIgnored => st.ignored += 1,
                    TrMetrics(mm) => {
                        let tname = test.name.as_slice();
                        let MetricMap(mm) = mm;
                        for (k,v) in mm.iter() {
                            st.metrics
                              .insert_metric(format!("{}.{}",
                                                     tname,
                                                     k).as_slice(),
                                             v.value,
                                             v.noise);
                        }
                        st.measured += 1
                    }
                    TrBench(bs) => {
                        st.metrics.insert_metric(test.name.as_slice(),
                                                 bs.ns_iter_summ.median,
                                                 bs.ns_iter_summ.max - bs.ns_iter_summ.min);
                        st.measured += 1
                    }
                    TrFailed => {
                        st.failed += 1;
                        st.failures.push((test, stdout));
                    }
                }
                Ok(())
            }
        }
    }

    let mut st = try!(ConsoleTestState::new(opts, None::<StdWriter>));
    fn len_if_padded(t: &TestDescAndFn) -> uint {
        match t.testfn.padding() {
            PadNone => 0u,
            PadOnLeft | PadOnRight => t.desc.name.as_slice().len(),
        }
    }
    match tests.iter().max_by(|t|len_if_padded(*t)) {
        Some(t) => {
            let n = t.desc.name.as_slice();
            st.max_name_len = n.len();
        },
        None => {}
    }
    try!(run_tests(opts, tests, |x| callback(&x, &mut st)));
    match opts.save_metrics {
        None => (),
        Some(ref pth) => {
            try!(st.metrics.save(pth));
            try!(st.write_plain(format!("\nmetrics saved to: {}",
                                          pth.display()).as_slice()));
        }
    }
    return st.write_run_finish(&opts.ratchet_metrics, opts.ratchet_noise_percent);
}

#[test]
fn should_sort_failures_before_printing_them() {
    use std::io::MemWriter;
    use std::str;

    let test_a = TestDesc {
        name: StaticTestName("a"),
        ignore: false,
        should_fail: false
    };

    let test_b = TestDesc {
        name: StaticTestName("b"),
        ignore: false,
        should_fail: false
    };

    let mut st = ConsoleTestState {
        log_out: None,
        out: Raw(MemWriter::new()),
        use_color: false,
        total: 0u,
        passed: 0u,
        failed: 0u,
        ignored: 0u,
        measured: 0u,
        max_name_len: 10u,
        metrics: MetricMap::new(),
        failures: vec!((test_b, Vec::new()), (test_a, Vec::new()))
    };

    st.write_failures().unwrap();
    let s = match st.out {
        Raw(ref m) => str::from_utf8_lossy(m.get_ref()),
        Pretty(_) => unreachable!()
    };

    let apos = s.as_slice().find_str("a").unwrap();
    let bpos = s.as_slice().find_str("b").unwrap();
    assert!(apos < bpos);
}

fn use_color(opts: &TestOpts) -> bool {
    match opts.color {
        AutoColor => get_concurrency() == 1 && io::stdout().get_ref().isatty(),
        AlwaysColor => true,
        NeverColor => false,
    }
}

#[deriving(Clone)]
enum TestEvent {
    TeFiltered(Vec<TestDesc> ),
    TeWait(TestDesc, NamePadding),
    TeResult(TestDesc, TestResult, Vec<u8> ),
}

pub type MonitorMsg = (TestDesc, TestResult, Vec<u8> );

fn run_tests(opts: &TestOpts,
             tests: Vec<TestDescAndFn> ,
             callback: |e: TestEvent| -> io::IoResult<()>) -> io::IoResult<()> {
    let filtered_tests = filter_tests(opts, tests);
    let filtered_descs = filtered_tests.iter()
                                       .map(|t| t.desc.clone())
                                       .collect();

    try!(callback(TeFiltered(filtered_descs)));

    let (filtered_tests, filtered_benchs_and_metrics) =
        filtered_tests.partition(|e| {
            match e.testfn {
                StaticTestFn(_) | DynTestFn(_) => true,
                _ => false
            }
        });

    // It's tempting to just spawn all the tests at once, but since we have
    // many tests that run in other processes we would be making a big mess.
    let concurrency = get_concurrency();

    let mut remaining = filtered_tests;
    remaining.reverse();
    let mut pending = 0;

    let (tx, rx) = channel::<MonitorMsg>();

    while pending > 0 || !remaining.is_empty() {
        while pending < concurrency && !remaining.is_empty() {
            let test = remaining.pop().unwrap();
            if concurrency == 1 {
                // We are doing one test at a time so we can print the name
                // of the test before we run it. Useful for debugging tests
                // that hang forever.
                try!(callback(TeWait(test.desc.clone(), test.testfn.padding())));
            }
            run_test(opts, !opts.run_tests, test, tx.clone());
            pending += 1;
        }

        let (desc, result, stdout) = rx.recv();
        if concurrency != 1 {
            try!(callback(TeWait(desc.clone(), PadNone)));
        }
        try!(callback(TeResult(desc, result, stdout)));
        pending -= 1;
    }

    // All benchmarks run at the end, in serial.
    // (this includes metric fns)
    for b in filtered_benchs_and_metrics.move_iter() {
        try!(callback(TeWait(b.desc.clone(), b.testfn.padding())));
        run_test(opts, !opts.run_benchmarks, b, tx.clone());
        let (test, result, stdout) = rx.recv();
        try!(callback(TeResult(test, result, stdout)));
    }
    Ok(())
}

fn get_concurrency() -> uint {
    use std::rt;
    match os::getenv("RUST_TEST_TASKS") {
        Some(s) => {
            let opt_n: Option<uint> = FromStr::from_str(s.as_slice());
            match opt_n {
                Some(n) if n > 0 => n,
                _ => fail!("RUST_TEST_TASKS is `{}`, should be a positive integer.", s)
            }
        }
        None => {
            rt::default_sched_threads()
        }
    }
}

pub fn filter_tests(opts: &TestOpts, tests: Vec<TestDescAndFn>) -> Vec<TestDescAndFn> {
    let mut filtered = tests;

    // Remove tests that don't match the test filter
    filtered = match opts.filter {
        None => filtered,
        Some(ref re) => {
            filtered.move_iter()
                .filter(|test| re.is_match(test.desc.name.as_slice())).collect()
        }
    };

    // Maybe pull out the ignored test and unignore them
    filtered = if !opts.run_ignored {
        filtered
    } else {
        fn filter(test: TestDescAndFn) -> Option<TestDescAndFn> {
            if test.desc.ignore {
                let TestDescAndFn {desc, testfn} = test;
                Some(TestDescAndFn {
                    desc: TestDesc {ignore: false, ..desc},
                    testfn: testfn
                })
            } else {
                None
            }
        };
        filtered.move_iter().filter_map(|x| filter(x)).collect()
    };

    // Sort the tests alphabetically
    filtered.sort_by(|t1, t2| t1.desc.name.as_slice().cmp(&t2.desc.name.as_slice()));

    // Shard the remaining tests, if sharding requested.
    match opts.test_shard {
        None => filtered,
        Some((a,b)) => {
            filtered.move_iter().enumerate()
            // note: using a - 1 so that the valid shards, for example, are
            // 1.2 and 2.2 instead of 0.2 and 1.2
            .filter(|&(i,_)| i % b == (a - 1))
            .map(|(_,t)| t)
            .collect()
        }
    }
}

pub fn run_test(opts: &TestOpts,
                force_ignore: bool,
                test: TestDescAndFn,
                monitor_ch: Sender<MonitorMsg>) {

    let TestDescAndFn {desc, testfn} = test;

    if force_ignore || desc.ignore {
        monitor_ch.send((desc, TrIgnored, Vec::new()));
        return;
    }

    fn run_test_inner(desc: TestDesc,
                      monitor_ch: Sender<MonitorMsg>,
                      nocapture: bool,
                      testfn: proc():Send) {
        spawn(proc() {
            let (tx, rx) = channel();
            let mut reader = ChanReader::new(rx);
            let stdout = ChanWriter::new(tx.clone());
            let stderr = ChanWriter::new(tx);
            let mut task = TaskBuilder::new().named(match desc.name {
                DynTestName(ref name) => name.clone().to_string(),
                StaticTestName(name) => name.to_string(),
            });
            if nocapture {
                drop((stdout, stderr));
            } else {
                task = task.stdout(box stdout as Box<Writer + Send>);
                task = task.stderr(box stderr as Box<Writer + Send>);
            }
            let result_future = task.try_future(testfn);

            let stdout = reader.read_to_end().unwrap().move_iter().collect();
            let task_result = result_future.unwrap();
            let test_result = calc_result(&desc, task_result.is_ok());
            monitor_ch.send((desc.clone(), test_result, stdout));
        })
    }

    match testfn {
        DynBenchFn(bencher) => {
            let bs = ::bench::benchmark(|harness| bencher.run(harness));
            monitor_ch.send((desc, TrBench(bs), Vec::new()));
            return;
        }
        StaticBenchFn(benchfn) => {
            let bs = ::bench::benchmark(|harness| benchfn(harness));
            monitor_ch.send((desc, TrBench(bs), Vec::new()));
            return;
        }
        DynMetricFn(f) => {
            let mut mm = MetricMap::new();
            f(&mut mm);
            monitor_ch.send((desc, TrMetrics(mm), Vec::new()));
            return;
        }
        StaticMetricFn(f) => {
            let mut mm = MetricMap::new();
            f(&mut mm);
            monitor_ch.send((desc, TrMetrics(mm), Vec::new()));
            return;
        }
        DynTestFn(f) => run_test_inner(desc, monitor_ch, opts.nocapture, f),
        StaticTestFn(f) => run_test_inner(desc, monitor_ch, opts.nocapture,
                                          proc() f())
    }
}

fn calc_result(desc: &TestDesc, task_succeeded: bool) -> TestResult {
    if task_succeeded {
        if desc.should_fail { TrFailed }
        else { TrOk }
    } else {
        if desc.should_fail { TrOk }
        else { TrFailed }
    }
}


impl ToJson for Metric {
    fn to_json(&self) -> json::Json {
        let mut map = TreeMap::new();
        map.insert("value".to_string(), json::Number(self.value));
        map.insert("noise".to_string(), json::Number(self.noise));
        json::Object(map)
    }
}


impl MetricMap {

    pub fn new() -> MetricMap {
        MetricMap(TreeMap::new())
    }

    /// Load MetricDiff from a file.
    ///
    /// # Failure
    ///
    /// This function will fail if the path does not exist or the path does not
    /// contain a valid metric map.
    pub fn load(p: &Path) -> MetricMap {
        assert!(p.exists());
        let mut f = File::open(p).unwrap();
        let value = json::from_reader(&mut f as &mut io::Reader).unwrap();
        let mut decoder = json::Decoder::new(value);
        MetricMap(match Decodable::decode(&mut decoder) {
            Ok(t) => t,
            Err(e) => fail!("failure decoding JSON: {}", e)
        })
    }

    /// Write MetricDiff to a file.
    pub fn save(&self, p: &Path) -> io::IoResult<()> {
        let mut file = try!(File::create(p));
        let MetricMap(ref map) = *self;

        // FIXME(pcwalton): Yuck.
        let mut new_map = TreeMap::new();
        for (ref key, ref value) in map.iter() {
            new_map.insert(key.to_string(), (*value).clone());
        }

        new_map.to_json().to_pretty_writer(&mut file)
    }

    /// Compare against another MetricMap. Optionally compare all
    /// measurements in the maps using the provided `noise_pct` as a
    /// percentage of each value to consider noise. If `None`, each
    /// measurement's noise threshold is independently chosen as the
    /// maximum of that measurement's recorded noise quantity in either
    /// map.
    pub fn compare_to_old(&self, old: &MetricMap,
                          noise_pct: Option<f64>) -> MetricDiff {
        let mut diff : MetricDiff = TreeMap::new();
        let MetricMap(ref selfmap) = *self;
        let MetricMap(ref old) = *old;
        for (k, vold) in old.iter() {
            let r = match selfmap.find(k) {
                None => MetricRemoved,
                Some(v) => {
                    let delta = v.value - vold.value;
                    let noise = match noise_pct {
                        None => vold.noise.abs().max(v.noise.abs()),
                        Some(pct) => vold.value * pct / 100.0
                    };
                    if delta.abs() <= noise {
                        LikelyNoise
                    } else {
                        let pct = delta.abs() / vold.value.max(f64::EPSILON) * 100.0;
                        if vold.noise < 0.0 {
                            // When 'noise' is negative, it means we want
                            // to see deltas that go up over time, and can
                            // only tolerate slight negative movement.
                            if delta < 0.0 {
                                Regression(pct)
                            } else {
                                Improvement(pct)
                            }
                        } else {
                            // When 'noise' is positive, it means we want
                            // to see deltas that go down over time, and
                            // can only tolerate slight positive movements.
                            if delta < 0.0 {
                                Improvement(pct)
                            } else {
                                Regression(pct)
                            }
                        }
                    }
                }
            };
            diff.insert((*k).clone(), r);
        }
        let MetricMap(ref map) = *self;
        for (k, _) in map.iter() {
            if !diff.contains_key(k) {
                diff.insert((*k).clone(), MetricAdded);
            }
        }
        diff
    }

    /// Insert a named `value` (+/- `noise`) metric into the map. The value
    /// must be non-negative. The `noise` indicates the uncertainty of the
    /// metric, which doubles as the "noise range" of acceptable
    /// pairwise-regressions on this named value, when comparing from one
    /// metric to the next using `compare_to_old`.
    ///
    /// If `noise` is positive, then it means this metric is of a value
    /// you want to see grow smaller, so a change larger than `noise` in the
    /// positive direction represents a regression.
    ///
    /// If `noise` is negative, then it means this metric is of a value
    /// you want to see grow larger, so a change larger than `noise` in the
    /// negative direction represents a regression.
    pub fn insert_metric(&mut self, name: &str, value: f64, noise: f64) {
        let m = Metric {
            value: value,
            noise: noise
        };
        let MetricMap(ref mut map) = *self;
        map.insert(name.to_string(), m);
    }

    /// Attempt to "ratchet" an external metric file. This involves loading
    /// metrics from a metric file (if it exists), comparing against
    /// the metrics in `self` using `compare_to_old`, and rewriting the
    /// file to contain the metrics in `self` if none of the
    /// `MetricChange`s are `Regression`. Returns the diff as well
    /// as a boolean indicating whether the ratchet succeeded.
    pub fn ratchet(&self, p: &Path, pct: Option<f64>) -> (MetricDiff, bool) {
        let old = if p.exists() {
            MetricMap::load(p)
        } else {
            MetricMap::new()
        };

        let diff : MetricDiff = self.compare_to_old(&old, pct);
        let ok = diff.iter().all(|(_, v)| {
            match *v {
                Regression(_) => false,
                _ => true
            }
        });

        if ok {
            self.save(p).unwrap();
        }
        return (diff, ok)
    }
}


// Benchmarking

/// A function that is opaque to the optimizer, to allow benchmarks to
/// pretend to use outputs to assist in avoiding dead-code
/// elimination.
///
/// This function is a no-op, and does not even read from `dummy`.
pub fn black_box<T>(dummy: T) {
    // we need to "use" the argument in some way LLVM can't
    // introspect.
    unsafe {asm!("" : : "r"(&dummy))}
}


impl Bencher {
    /// Callback for benchmark functions to run in their body.
    pub fn iter<T>(&mut self, inner: || -> T) {
        self.ns_start = precise_time_ns();
        let k = self.iterations;
        for _ in range(0u64, k) {
            black_box(inner());
        }
        self.ns_end = precise_time_ns();
    }

    pub fn ns_elapsed(&mut self) -> u64 {
        if self.ns_start == 0 || self.ns_end == 0 {
            0
        } else {
            self.ns_end - self.ns_start
        }
    }

    pub fn ns_per_iter(&mut self) -> u64 {
        if self.iterations == 0 {
            0
        } else {
            self.ns_elapsed() / cmp::max(self.iterations, 1)
        }
    }

    pub fn bench_n(&mut self, n: u64, f: |&mut Bencher|) {
        self.iterations = n;
        f(self);
    }

    // This is a more statistics-driven benchmark algorithm
    pub fn auto_bench(&mut self, f: |&mut Bencher|) -> stats::Summary<f64> {

        // Initial bench run to get ballpark figure.
        let mut n = 1_u64;
        self.bench_n(n, |x| f(x));

        // Try to estimate iter count for 1ms falling back to 1m
        // iterations if first run took < 1ns.
        if self.ns_per_iter() == 0 {
            n = 1_000_000;
        } else {
            n = 1_000_000 / cmp::max(self.ns_per_iter(), 1);
        }
        // if the first run took more than 1ms we don't want to just
        // be left doing 0 iterations on every loop. The unfortunate
        // side effect of not being able to do as many runs is
        // automatically handled by the statistical analysis below
        // (i.e. larger error bars).
        if n == 0 { n = 1; }

        let mut total_run = 0;
        let samples : &mut [f64] = [0.0_f64, ..50];
        loop {
            let loop_start = precise_time_ns();

            for p in samples.mut_iter() {
                self.bench_n(n, |x| f(x));
                *p = self.ns_per_iter() as f64;
            };

            stats::winsorize(samples, 5.0);
            let summ = stats::Summary::new(samples);

            for p in samples.mut_iter() {
                self.bench_n(5 * n, |x| f(x));
                *p = self.ns_per_iter() as f64;
            };

            stats::winsorize(samples, 5.0);
            let summ5 = stats::Summary::new(samples);

            let now = precise_time_ns();
            let loop_run = now - loop_start;

            // If we've run for 100ms and seem to have converged to a
            // stable median.
            if loop_run > 100_000_000 &&
                summ.median_abs_dev_pct < 1.0 &&
                summ.median - summ5.median < summ5.median_abs_dev {
                return summ5;
            }

            total_run += loop_run;
            // Longest we ever run for is 3s.
            if total_run > 3_000_000_000 {
                return summ5;
            }

            n *= 2;
        }
    }
}

pub mod bench {
    use std::cmp;
    use super::{Bencher, BenchSamples};

    pub fn benchmark(f: |&mut Bencher|) -> BenchSamples {
        let mut bs = Bencher {
            iterations: 0,
            ns_start: 0,
            ns_end: 0,
            bytes: 0
        };

        let ns_iter_summ = bs.auto_bench(f);

        let ns_iter = cmp::max(ns_iter_summ.median as u64, 1);
        let iter_s = 1_000_000_000 / ns_iter;
        let mb_s = (bs.bytes * iter_s) / 1_000_000;

        BenchSamples {
            ns_iter_summ: ns_iter_summ,
            mb_s: mb_s as uint
        }
    }
}

#[cfg(test)]
mod tests {
    use test::{TrFailed, TrIgnored, TrOk, filter_tests, parse_opts,
               TestDesc, TestDescAndFn, TestOpts, run_test,
               Metric, MetricMap, MetricAdded, MetricRemoved,
               Improvement, Regression, LikelyNoise,
               StaticTestName, DynTestName, DynTestFn};
    use std::io::TempDir;

    #[test]
    pub fn do_not_run_ignored_tests() {
        fn f() { fail!(); }
        let desc = TestDescAndFn {
            desc: TestDesc {
                name: StaticTestName("whatever"),
                ignore: true,
                should_fail: false
            },
            testfn: DynTestFn(proc() f()),
        };
        let (tx, rx) = channel();
        run_test(&TestOpts::new(), false, desc, tx);
        let (_, res, _) = rx.recv();
        assert!(res != TrOk);
    }

    #[test]
    pub fn ignored_tests_result_in_ignored() {
        fn f() { }
        let desc = TestDescAndFn {
            desc: TestDesc {
                name: StaticTestName("whatever"),
                ignore: true,
                should_fail: false
            },
            testfn: DynTestFn(proc() f()),
        };
        let (tx, rx) = channel();
        run_test(&TestOpts::new(), false, desc, tx);
        let (_, res, _) = rx.recv();
        assert!(res == TrIgnored);
    }

    #[test]
    fn test_should_fail() {
        fn f() { fail!(); }
        let desc = TestDescAndFn {
            desc: TestDesc {
                name: StaticTestName("whatever"),
                ignore: false,
                should_fail: true
            },
            testfn: DynTestFn(proc() f()),
        };
        let (tx, rx) = channel();
        run_test(&TestOpts::new(), false, desc, tx);
        let (_, res, _) = rx.recv();
        assert!(res == TrOk);
    }

    #[test]
    fn test_should_fail_but_succeeds() {
        fn f() { }
        let desc = TestDescAndFn {
            desc: TestDesc {
                name: StaticTestName("whatever"),
                ignore: false,
                should_fail: true
            },
            testfn: DynTestFn(proc() f()),
        };
        let (tx, rx) = channel();
        run_test(&TestOpts::new(), false, desc, tx);
        let (_, res, _) = rx.recv();
        assert!(res == TrFailed);
    }

    #[test]
    fn first_free_arg_should_be_a_filter() {
        let args = vec!("progname".to_string(), "some_regex_filter".to_string());
        let opts = match parse_opts(args.as_slice()) {
            Some(Ok(o)) => o,
            _ => fail!("Malformed arg in first_free_arg_should_be_a_filter")
        };
        assert!(opts.filter.expect("should've found filter").is_match("some_regex_filter"))
    }

    #[test]
    fn parse_ignored_flag() {
        let args = vec!("progname".to_string(),
                        "filter".to_string(),
                        "--ignored".to_string());
        let opts = match parse_opts(args.as_slice()) {
            Some(Ok(o)) => o,
            _ => fail!("Malformed arg in parse_ignored_flag")
        };
        assert!((opts.run_ignored));
    }

    #[test]
    pub fn filter_for_ignored_option() {
        // When we run ignored tests the test filter should filter out all the
        // unignored tests and flip the ignore flag on the rest to false

        let mut opts = TestOpts::new();
        opts.run_tests = true;
        opts.run_ignored = true;

        let tests = vec!(
            TestDescAndFn {
                desc: TestDesc {
                    name: StaticTestName("1"),
                    ignore: true,
                    should_fail: false,
                },
                testfn: DynTestFn(proc() {}),
            },
            TestDescAndFn {
                desc: TestDesc {
                    name: StaticTestName("2"),
                    ignore: false,
                    should_fail: false
                },
                testfn: DynTestFn(proc() {}),
            });
        let filtered = filter_tests(&opts, tests);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered.get(0).desc.name.to_str(),
                   "1".to_string());
        assert!(filtered.get(0).desc.ignore == false);
    }

    #[test]
    pub fn sort_tests() {
        let mut opts = TestOpts::new();
        opts.run_tests = true;

        let names =
            vec!("sha1::test".to_string(),
                 "int::test_to_str".to_string(),
                 "int::test_pow".to_string(),
                 "test::do_not_run_ignored_tests".to_string(),
                 "test::ignored_tests_result_in_ignored".to_string(),
                 "test::first_free_arg_should_be_a_filter".to_string(),
                 "test::parse_ignored_flag".to_string(),
                 "test::filter_for_ignored_option".to_string(),
                 "test::sort_tests".to_string());
        let tests =
        {
            fn testfn() { }
            let mut tests = Vec::new();
            for name in names.iter() {
                let test = TestDescAndFn {
                    desc: TestDesc {
                        name: DynTestName((*name).clone()),
                        ignore: false,
                        should_fail: false
                    },
                    testfn: DynTestFn(testfn),
                };
                tests.push(test);
            }
            tests
        };
        let filtered = filter_tests(&opts, tests);

        let expected =
            vec!("int::test_pow".to_string(),
                 "int::test_to_str".to_string(),
                 "sha1::test".to_string(),
                 "test::do_not_run_ignored_tests".to_string(),
                 "test::filter_for_ignored_option".to_string(),
                 "test::first_free_arg_should_be_a_filter".to_string(),
                 "test::ignored_tests_result_in_ignored".to_string(),
                 "test::parse_ignored_flag".to_string(),
                 "test::sort_tests".to_string());

        for (a, b) in expected.iter().zip(filtered.iter()) {
            assert!(*a == b.desc.name.to_str());
        }
    }

    #[test]
    pub fn filter_tests_regex() {
        let mut opts = TestOpts::new();
        opts.filter = Some(::regex::Regex::new("a.*b.+c").unwrap());

        let mut names = ["yes::abXc", "yes::aXXXbXXXXc",
                         "no::XYZ", "no::abc"];
        names.sort();

        fn test_fn() {}
        let tests = names.iter().map(|name| {
            TestDescAndFn {
                desc: TestDesc {
                    name: DynTestName(name.to_string()),
                    ignore: false,
                    should_fail: false
                },
                testfn: DynTestFn(test_fn)
            }
        }).collect();
        let filtered = filter_tests(&opts, tests);

        let expected: Vec<&str> =
            names.iter().map(|&s| s).filter(|name| name.starts_with("yes")).collect();

        assert_eq!(filtered.len(), expected.len());
        for (test, expected_name) in filtered.iter().zip(expected.iter()) {
            assert_eq!(test.desc.name.as_slice(), *expected_name);
        }
    }

    #[test]
    pub fn test_metricmap_compare() {
        let mut m1 = MetricMap::new();
        let mut m2 = MetricMap::new();
        m1.insert_metric("in-both-noise", 1000.0, 200.0);
        m2.insert_metric("in-both-noise", 1100.0, 200.0);

        m1.insert_metric("in-first-noise", 1000.0, 2.0);
        m2.insert_metric("in-second-noise", 1000.0, 2.0);

        m1.insert_metric("in-both-want-downwards-but-regressed", 1000.0, 10.0);
        m2.insert_metric("in-both-want-downwards-but-regressed", 2000.0, 10.0);

        m1.insert_metric("in-both-want-downwards-and-improved", 2000.0, 10.0);
        m2.insert_metric("in-both-want-downwards-and-improved", 1000.0, 10.0);

        m1.insert_metric("in-both-want-upwards-but-regressed", 2000.0, -10.0);
        m2.insert_metric("in-both-want-upwards-but-regressed", 1000.0, -10.0);

        m1.insert_metric("in-both-want-upwards-and-improved", 1000.0, -10.0);
        m2.insert_metric("in-both-want-upwards-and-improved", 2000.0, -10.0);

        let diff1 = m2.compare_to_old(&m1, None);

        assert_eq!(*(diff1.find(&"in-both-noise".to_string()).unwrap()), LikelyNoise);
        assert_eq!(*(diff1.find(&"in-first-noise".to_string()).unwrap()), MetricRemoved);
        assert_eq!(*(diff1.find(&"in-second-noise".to_string()).unwrap()), MetricAdded);
        assert_eq!(*(diff1.find(&"in-both-want-downwards-but-regressed".to_string()).unwrap()),
                   Regression(100.0));
        assert_eq!(*(diff1.find(&"in-both-want-downwards-and-improved".to_string()).unwrap()),
                   Improvement(50.0));
        assert_eq!(*(diff1.find(&"in-both-want-upwards-but-regressed".to_string()).unwrap()),
                   Regression(50.0));
        assert_eq!(*(diff1.find(&"in-both-want-upwards-and-improved".to_string()).unwrap()),
                   Improvement(100.0));
        assert_eq!(diff1.len(), 7);

        let diff2 = m2.compare_to_old(&m1, Some(200.0));

        assert_eq!(*(diff2.find(&"in-both-noise".to_string()).unwrap()), LikelyNoise);
        assert_eq!(*(diff2.find(&"in-first-noise".to_string()).unwrap()), MetricRemoved);
        assert_eq!(*(diff2.find(&"in-second-noise".to_string()).unwrap()), MetricAdded);
        assert_eq!(*(diff2.find(&"in-both-want-downwards-but-regressed".to_string()).unwrap()),
                   LikelyNoise);
        assert_eq!(*(diff2.find(&"in-both-want-downwards-and-improved".to_string()).unwrap()),
                   LikelyNoise);
        assert_eq!(*(diff2.find(&"in-both-want-upwards-but-regressed".to_string()).unwrap()),
                   LikelyNoise);
        assert_eq!(*(diff2.find(&"in-both-want-upwards-and-improved".to_string()).unwrap()),
                   LikelyNoise);
        assert_eq!(diff2.len(), 7);
    }

    #[test]
    pub fn ratchet_test() {

        let dpth = TempDir::new("test-ratchet").expect("missing test for ratchet");
        let pth = dpth.path().join("ratchet.json");

        let mut m1 = MetricMap::new();
        m1.insert_metric("runtime", 1000.0, 2.0);
        m1.insert_metric("throughput", 50.0, 2.0);

        let mut m2 = MetricMap::new();
        m2.insert_metric("runtime", 1100.0, 2.0);
        m2.insert_metric("throughput", 50.0, 2.0);

        m1.save(&pth).unwrap();

        // Ask for a ratchet that should fail to advance.
        let (diff1, ok1) = m2.ratchet(&pth, None);
        assert_eq!(ok1, false);
        assert_eq!(diff1.len(), 2);
        assert_eq!(*(diff1.find(&"runtime".to_string()).unwrap()), Regression(10.0));
        assert_eq!(*(diff1.find(&"throughput".to_string()).unwrap()), LikelyNoise);

        // Check that it was not rewritten.
        let m3 = MetricMap::load(&pth);
        let MetricMap(m3) = m3;
        assert_eq!(m3.len(), 2);
        assert_eq!(*(m3.find(&"runtime".to_string()).unwrap()), Metric::new(1000.0, 2.0));
        assert_eq!(*(m3.find(&"throughput".to_string()).unwrap()), Metric::new(50.0, 2.0));

        // Ask for a ratchet with an explicit noise-percentage override,
        // that should advance.
        let (diff2, ok2) = m2.ratchet(&pth, Some(10.0));
        assert_eq!(ok2, true);
        assert_eq!(diff2.len(), 2);
        assert_eq!(*(diff2.find(&"runtime".to_string()).unwrap()), LikelyNoise);
        assert_eq!(*(diff2.find(&"throughput".to_string()).unwrap()), LikelyNoise);

        // Check that it was rewritten.
        let m4 = MetricMap::load(&pth);
        let MetricMap(m4) = m4;
        assert_eq!(m4.len(), 2);
        assert_eq!(*(m4.find(&"runtime".to_string()).unwrap()), Metric::new(1100.0, 2.0));
        assert_eq!(*(m4.find(&"throughput".to_string()).unwrap()), Metric::new(50.0, 2.0));
    }
}
