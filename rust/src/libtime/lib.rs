// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Simple time handling.

#![crate_name = "time"]
#![experimental]

#![crate_type = "rlib"]
#![crate_type = "dylib"]
#![license = "MIT/ASL2"]
#![doc(html_logo_url = "http://www.rust-lang.org/logos/rust-logo-128x128-blk-v2.png",
       html_favicon_url = "http://www.rust-lang.org/favicon.ico",
       html_root_url = "http://doc.rust-lang.org/nightly/",
       html_playground_url = "http://play.rust-lang.org/")]
#![feature(phase)]

#[cfg(test)] #[phase(plugin, link)] extern crate log;

extern crate serialize;
extern crate libc;

use std::fmt::Show;
use std::fmt;
use std::io::BufReader;
use std::num;
use std::string::String;
use std::time::Duration;

static NSEC_PER_SEC: i32 = 1_000_000_000_i32;

mod rustrt {
    use super::Tm;

    extern {
        pub fn rust_tzset();
        pub fn rust_gmtime(sec: i64, nsec: i32, result: &mut Tm);
        pub fn rust_localtime(sec: i64, nsec: i32, result: &mut Tm);
        pub fn rust_timegm(tm: &Tm) -> i64;
        pub fn rust_mktime(tm: &Tm) -> i64;
    }
}

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios")))]
mod imp {
    use libc::{c_int, timespec};

    // Apparently android provides this in some other library?
    #[cfg(not(target_os = "android"))]
    #[link(name = "rt")]
    extern {}

    extern {
        pub fn clock_gettime(clk_id: c_int, tp: *mut timespec) -> c_int;
    }

}
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod imp {
    use libc::{timeval, timezone, c_int, mach_timebase_info};

    extern {
        pub fn gettimeofday(tp: *mut timeval, tzp: *mut timezone) -> c_int;
        pub fn mach_absolute_time() -> u64;
        pub fn mach_timebase_info(info: *mut mach_timebase_info) -> c_int;
    }
}

/// A record specifying a time value in seconds and nanoseconds.
#[deriving(Clone, PartialEq, Eq, PartialOrd, Ord, Encodable, Decodable, Show)]
pub struct Timespec { pub sec: i64, pub nsec: i32 }
/*
 * Timespec assumes that pre-epoch Timespecs have negative sec and positive
 * nsec fields. Darwin's and Linux's struct timespec functions handle pre-
 * epoch timestamps using a "two steps back, one step forward" representation,
 * though the man pages do not actually document this. For example, the time
 * -1.2 seconds before the epoch is represented by `Timespec { sec: -2_i64,
 * nsec: 800_000_000_i32 }`.
 */
impl Timespec {
    pub fn new(sec: i64, nsec: i32) -> Timespec {
        assert!(nsec >= 0 && nsec < NSEC_PER_SEC);
        Timespec { sec: sec, nsec: nsec }
    }
}

impl Add<Duration, Timespec> for Timespec {
    fn add(&self, other: &Duration) -> Timespec {
        let d_sec = other.num_seconds();
        // It is safe to unwrap the nanoseconds, because there cannot be
        // more than one second left, which fits in i64 and in i32.
        let d_nsec = (*other - Duration::seconds(d_sec))
                     .num_nanoseconds().unwrap() as i32;
        let mut sec = self.sec + d_sec;
        let mut nsec = self.nsec + d_nsec;
        if nsec >= NSEC_PER_SEC {
            nsec -= NSEC_PER_SEC;
            sec += 1;
        } else if nsec < 0 {
            nsec += NSEC_PER_SEC;
            sec -= 1;
        }
        Timespec::new(sec, nsec)
    }
}

impl Sub<Timespec, Duration> for Timespec {
    fn sub(&self, other: &Timespec) -> Duration {
        let sec = self.sec - other.sec;
        let nsec = self.nsec - other.nsec;
        Duration::seconds(sec) + Duration::nanoseconds(nsec as i64)
    }
}

/**
 * Returns the current time as a `timespec` containing the seconds and
 * nanoseconds since 1970-01-01T00:00:00Z.
 */
pub fn get_time() -> Timespec {
    unsafe {
        let (sec, nsec) = os_get_time();
        return Timespec::new(sec, nsec);
    }

    #[cfg(windows)]
    unsafe fn os_get_time() -> (i64, i32) {
        static NANOSECONDS_FROM_1601_TO_1970: u64 = 11644473600000000;

        let mut time = libc::FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        libc::GetSystemTimeAsFileTime(&mut time);

        // A FILETIME contains a 64-bit value representing the number of
        // hectonanosecond (100-nanosecond) intervals since 1601-01-01T00:00:00Z.
        // http://support.microsoft.com/kb/167296/en-us
        let ns_since_1601 = ((time.dwHighDateTime as u64 << 32) |
                             (time.dwLowDateTime  as u64 <<  0)) / 10;
        let ns_since_1970 = ns_since_1601 - NANOSECONDS_FROM_1601_TO_1970;

        ((ns_since_1970 / 1000000) as i64,
         ((ns_since_1970 % 1000000) * 1000) as i32)
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    unsafe fn os_get_time() -> (i64, i32) {
        use std::ptr;
        let mut tv = libc::timeval { tv_sec: 0, tv_usec: 0 };
        imp::gettimeofday(&mut tv, ptr::null_mut());
        (tv.tv_sec as i64, tv.tv_usec * 1000)
    }

    #[cfg(not(any(target_os = "macos", target_os = "ios", windows)))]
    unsafe fn os_get_time() -> (i64, i32) {
        let mut tv = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        imp::clock_gettime(libc::CLOCK_REALTIME, &mut tv);
        (tv.tv_sec as i64, tv.tv_nsec as i32)
    }
}


/**
 * Returns the current value of a high-resolution performance counter
 * in nanoseconds since an unspecified epoch.
 */
pub fn precise_time_ns() -> u64 {
    return os_precise_time_ns();

    #[cfg(windows)]
    fn os_precise_time_ns() -> u64 {
        let mut ticks_per_s = 0;
        assert_eq!(unsafe {
            libc::QueryPerformanceFrequency(&mut ticks_per_s)
        }, 1);
        let ticks_per_s = if ticks_per_s == 0 {1} else {ticks_per_s};
        let mut ticks = 0;
        assert_eq!(unsafe {
            libc::QueryPerformanceCounter(&mut ticks)
        }, 1);

        return (ticks as u64 * 1000000000) / (ticks_per_s as u64);
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    fn os_precise_time_ns() -> u64 {
        static mut TIMEBASE: libc::mach_timebase_info = libc::mach_timebase_info { numer: 0,
                                                                                   denom: 0 };
        static ONCE: std::sync::Once = std::sync::ONCE_INIT;
        unsafe {
            ONCE.doit(|| {
                imp::mach_timebase_info(&mut TIMEBASE);
            });
            let time = imp::mach_absolute_time();
            time * TIMEBASE.numer as u64 / TIMEBASE.denom as u64
        }
    }

    #[cfg(not(any(windows, target_os = "macos", target_os = "ios")))]
    fn os_precise_time_ns() -> u64 {
        let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        unsafe {
            imp::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
        }
        return (ts.tv_sec as u64) * 1000000000 + (ts.tv_nsec as u64)
    }
}


/**
 * Returns the current value of a high-resolution performance counter
 * in seconds since an unspecified epoch.
 */
pub fn precise_time_s() -> f64 {
    return (precise_time_ns() as f64) / 1000000000.;
}

pub fn tzset() {
    unsafe {
        rustrt::rust_tzset();
    }
}

/// Holds a calendar date and time broken down into its components (year, month, day, and so on),
/// also called a broken-down time value.
// FIXME: use c_int instead of i32?
#[repr(C)]
#[deriving(Clone, PartialEq, Eq, Show)]
pub struct Tm {
    /// Seconds after the minute - [0, 60]
    pub tm_sec: i32,

    /// Minutes after the hour - [0, 59]
    pub tm_min: i32,

    /// Hours after midnight - [0, 23]
    pub tm_hour: i32,

    /// Day of the month - [1, 31]
    pub tm_mday: i32,

    /// Months since January - [0, 11]
    pub tm_mon: i32,

    /// Years since 1900
    pub tm_year: i32,

    /// Days since Sunday - [0, 6]. 0 = Sunday, 1 = Monday, ..., 6 = Saturday.
    pub tm_wday: i32,

    /// Days since January 1 - [0, 365]
    pub tm_yday: i32,

    /// Daylight Saving Time flag.
    ///
    /// This value is positive if Daylight Saving Time is in effect, zero if Daylight Saving Time
    /// is not in effect, and negative if this information is not available.
    pub tm_isdst: i32,

    /// Identifies the time zone that was used to compute this broken-down time value, including any
    /// adjustment for Daylight Saving Time. This is the number of seconds east of UTC. For example,
    /// for U.S. Pacific Daylight Time, the value is -7*60*60 = -25200.
    pub tm_gmtoff: i32,

    /// Nanoseconds after the second - [0, 10<sup>9</sup> - 1]
    pub tm_nsec: i32,
}

pub fn empty_tm() -> Tm {
    Tm {
        tm_sec: 0_i32,
        tm_min: 0_i32,
        tm_hour: 0_i32,
        tm_mday: 0_i32,
        tm_mon: 0_i32,
        tm_year: 0_i32,
        tm_wday: 0_i32,
        tm_yday: 0_i32,
        tm_isdst: 0_i32,
        tm_gmtoff: 0_i32,
        tm_nsec: 0_i32,
    }
}

/// Returns the specified time in UTC
pub fn at_utc(clock: Timespec) -> Tm {
    unsafe {
        let Timespec { sec, nsec } = clock;
        let mut tm = empty_tm();
        rustrt::rust_gmtime(sec, nsec, &mut tm);
        tm
    }
}

/// Returns the current time in UTC
pub fn now_utc() -> Tm {
    at_utc(get_time())
}

/// Returns the specified time in the local timezone
pub fn at(clock: Timespec) -> Tm {
    unsafe {
        let Timespec { sec, nsec } = clock;
        let mut tm = empty_tm();
        rustrt::rust_localtime(sec, nsec, &mut tm);
        tm
    }
}

/// Returns the current time in the local timezone
pub fn now() -> Tm {
    at(get_time())
}

impl Tm {
    /// Convert time to the seconds from January 1, 1970
    pub fn to_timespec(&self) -> Timespec {
        unsafe {
            let sec = match self.tm_gmtoff {
                0_i32 => rustrt::rust_timegm(self),
                _     => rustrt::rust_mktime(self)
            };

            Timespec::new(sec, self.tm_nsec)
        }
    }

    /// Convert time to the local timezone
    pub fn to_local(&self) -> Tm {
        at(self.to_timespec())
    }

    /// Convert time to the UTC
    pub fn to_utc(&self) -> Tm {
        at_utc(self.to_timespec())
    }

    /**
     * Returns a TmFmt that outputs according to the `asctime` format in ISO
     * C, in the local timezone.
     *
     * Example: "Thu Jan  1 00:00:00 1970"
     */
    pub fn ctime(&self) -> TmFmt {
        TmFmt {
            tm: self,
            format: FmtCtime,
        }
    }

    /**
     * Returns a TmFmt that outputs according to the `asctime` format in ISO
     * C.
     *
     * Example: "Thu Jan  1 00:00:00 1970"
     */
    pub fn asctime(&self) -> TmFmt {
        TmFmt {
            tm: self,
            format: FmtStr("%c"),
        }
    }

    /// Formats the time according to the format string.
    pub fn strftime<'a>(&'a self, format: &'a str) -> Result<TmFmt<'a>, ParseError> {
        validate_format(TmFmt {
            tm: self,
            format: FmtStr(format),
        })
    }

    /**
     * Returns a TmFmt that outputs according to RFC 822.
     *
     * local: "Thu, 22 Mar 2012 07:53:18 PST"
     * utc:   "Thu, 22 Mar 2012 14:53:18 GMT"
     */
    pub fn rfc822(&self) -> TmFmt {
        if self.tm_gmtoff == 0_i32 {
            TmFmt {
                tm: self,
                format: FmtStr("%a, %d %b %Y %T GMT"),
            }
        } else {
            TmFmt {
                tm: self,
                format: FmtStr("%a, %d %b %Y %T %Z"),
            }
        }
    }

    /**
     * Returns a TmFmt that outputs according to RFC 822 with Zulu time.
     *
     * local: "Thu, 22 Mar 2012 07:53:18 -0700"
     * utc:   "Thu, 22 Mar 2012 14:53:18 -0000"
     */
    pub fn rfc822z(&self) -> TmFmt {
        TmFmt {
            tm: self,
            format: FmtStr("%a, %d %b %Y %T %z"),
        }
    }

    /**
     * Returns a TmFmt that outputs according to RFC 3339. RFC 3339 is
     * compatible with ISO 8601.
     *
     * local: "2012-02-22T07:53:18-07:00"
     * utc:   "2012-02-22T14:53:18Z"
     */
    pub fn rfc3339<'a>(&'a self) -> TmFmt {
        TmFmt {
            tm: self,
            format: FmtRfc3339,
        }
    }
}

#[deriving(PartialEq)]
pub enum ParseError {
    InvalidSecond,
    InvalidMinute,
    InvalidHour,
    InvalidDay,
    InvalidMonth,
    InvalidYear,
    InvalidDayOfWeek,
    InvalidDayOfMonth,
    InvalidDayOfYear,
    InvalidZoneOffset,
    InvalidTime,
    MissingFormatConverter,
    InvalidFormatSpecifier(char),
    UnexpectedCharacter(char, char),
}

impl Show for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            InvalidSecond => write!(f, "Invalid second."),
            InvalidMinute => write!(f, "Invalid minute."),
            InvalidHour => write!(f, "Invalid hour."),
            InvalidDay => write!(f, "Invalid day."),
            InvalidMonth => write!(f, "Invalid month."),
            InvalidYear => write!(f, "Invalid year."),
            InvalidDayOfWeek => write!(f, "Invalid day of the week."),
            InvalidDayOfMonth => write!(f, "Invalid day of the month."),
            InvalidDayOfYear => write!(f, "Invalid day of the year."),
            InvalidZoneOffset => write!(f, "Invalid zone offset."),
            InvalidTime => write!(f, "Invalid time."),
            MissingFormatConverter => write!(f, "Missing format converter after `%`"),
            InvalidFormatSpecifier(ch) => write!(f, "Invalid format specifier: %{}", ch),
            UnexpectedCharacter(a, b) => write!(f, "Expected: {}, found: {}.", a, b),
        }
    }
}

/// A wrapper around a `Tm` and format string that implements Show.
pub struct TmFmt<'a> {
    tm: &'a Tm,
    format: Fmt<'a>
}

enum Fmt<'a> {
    FmtStr(&'a str),
    FmtRfc3339,
    FmtCtime,
}

fn validate_format<'a>(fmt: TmFmt<'a>) -> Result<TmFmt<'a>, ParseError> {

    match (fmt.tm.tm_wday, fmt.tm.tm_mon) {
        (0...6, 0...11) => (),
        (_wday, 0...11) => return Err(InvalidDayOfWeek),
        (0...6, _mon) => return Err(InvalidMonth),
        _ => return Err(InvalidDay)
    }
    match fmt.format {
        FmtStr(ref s) => {
            let mut chars = s.chars();
            loop {
                match chars.next() {
                    Some('%') => {
                        match chars.next() {
                            Some('A') |
                            Some('a') |
                            Some('B') |
                            Some('b') |
                            Some('C') |
                            Some('c') |
                            Some('D') |
                            Some('d') |
                            Some('e') |
                            Some('F') |
                            Some('f') |
                            Some('G') |
                            Some('g') |
                            Some('H') |
                            Some('h') |
                            Some('I') |
                            Some('j') |
                            Some('k') |
                            Some('l') |
                            Some('M') |
                            Some('m') |
                            Some('n') |
                            Some('P') |
                            Some('p') |
                            Some('R') |
                            Some('r') |
                            Some('S') |
                            Some('s') |
                            Some('T') |
                            Some('t') |
                            Some('U') |
                            Some('u') |
                            Some('V') |
                            Some('v') |
                            Some('W') |
                            Some('w') |
                            Some('X') |
                            Some('x') |
                            Some('Y') |
                            Some('y') |
                            Some('Z') |
                            Some('z') |
                            Some('+') |
                            Some('%')
                                => (),
                            Some(c) => return Err(InvalidFormatSpecifier(c)),
                            None => return Err(MissingFormatConverter),
                        }
                    },
                    None => break,
                    _ => ()
                }
            }
        },
        _ => ()
    }
    Ok(fmt)
}

impl<'a> fmt::Show for TmFmt<'a> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fn days_in_year(year: int) -> i32 {
            if (year % 4 == 0) && ((year % 100 != 0) || (year % 400 == 0)) {
                366    /* Days in a leap year */
            } else {
                365    /* Days in a non-leap year */
            }
        }

        fn iso_week_days(yday: i32, wday: i32) -> int {
            /* The number of days from the first day of the first ISO week of this
            * year to the year day YDAY with week day WDAY.
            * ISO weeks start on Monday. The first ISO week has the year's first
            * Thursday.
            * YDAY may be as small as yday_minimum.
            */
            let yday: int = yday as int;
            let wday: int = wday as int;
            let iso_week_start_wday: int = 1;                     /* Monday */
            let iso_week1_wday: int = 4;                          /* Thursday */
            let yday_minimum: int = 366;
            /* Add enough to the first operand of % to make it nonnegative. */
            let big_enough_multiple_of_7: int = (yday_minimum / 7 + 2) * 7;

            yday - (yday - wday + iso_week1_wday + big_enough_multiple_of_7) % 7
                + iso_week1_wday - iso_week_start_wday
        }

        fn iso_week(fmt: &mut fmt::Formatter, ch:char, tm: &Tm) -> fmt::Result {
            let mut year: int = tm.tm_year as int + 1900;
            let mut days: int = iso_week_days (tm.tm_yday, tm.tm_wday);

            if days < 0 {
                /* This ISO week belongs to the previous year. */
                year -= 1;
                days = iso_week_days (tm.tm_yday + (days_in_year(year)), tm.tm_wday);
            } else {
                let d: int = iso_week_days (tm.tm_yday - (days_in_year(year)),
                                            tm.tm_wday);
                if 0 <= d {
                    /* This ISO week belongs to the next year. */
                    year += 1;
                    days = d;
                }
            }

            match ch {
                'G' => write!(fmt, "{}", year),
                'g' => write!(fmt, "{:02d}", (year % 100 + 100) % 100),
                'V' => write!(fmt, "{:02d}", days / 7 + 1),
                _ => Ok(())
            }
        }

        fn parse_type(fmt: &mut fmt::Formatter, ch: char, tm: &Tm) -> fmt::Result {
            let die = || {
                unreachable!()
            };
            match ch {
              'A' => match tm.tm_wday as int {
                0 => "Sunday",
                1 => "Monday",
                2 => "Tuesday",
                3 => "Wednesday",
                4 => "Thursday",
                5 => "Friday",
                6 => "Saturday",
                _ => return die()
              },
             'a' => match tm.tm_wday as int {
                0 => "Sun",
                1 => "Mon",
                2 => "Tue",
                3 => "Wed",
                4 => "Thu",
                5 => "Fri",
                6 => "Sat",
                _ => return die()
              },
              'B' => match tm.tm_mon as int {
                0 => "January",
                1 => "February",
                2 => "March",
                3 => "April",
                4 => "May",
                5 => "June",
                6 => "July",
                7 => "August",
                8 => "September",
                9 => "October",
                10 => "November",
                11 => "December",
                _ => return die()
              },
              'b' | 'h' => match tm.tm_mon as int {
                0 => "Jan",
                1 => "Feb",
                2 => "Mar",
                3 => "Apr",
                4 => "May",
                5 => "Jun",
                6 => "Jul",
                7 => "Aug",
                8 => "Sep",
                9 => "Oct",
                10 => "Nov",
                11 => "Dec",
                _  => return die()
              },
              'C' => return write!(fmt, "{:02d}", (tm.tm_year as int + 1900) / 100),
              'c' => {
                    try!(parse_type(fmt, 'a', tm));
                    try!(' '.fmt(fmt));
                    try!(parse_type(fmt, 'b', tm));
                    try!(' '.fmt(fmt));
                    try!(parse_type(fmt, 'e', tm));
                    try!(' '.fmt(fmt));
                    try!(parse_type(fmt, 'T', tm));
                    try!(' '.fmt(fmt));
                    return parse_type(fmt, 'Y', tm);
              }
              'D' | 'x' => {
                    try!(parse_type(fmt, 'm', tm));
                    try!('/'.fmt(fmt));
                    try!(parse_type(fmt, 'd', tm));
                    try!('/'.fmt(fmt));
                    return parse_type(fmt, 'y', tm);
              }
              'd' => return write!(fmt, "{:02d}", tm.tm_mday),
              'e' => return write!(fmt, "{:2d}", tm.tm_mday),
              'f' => return write!(fmt, "{:09d}", tm.tm_nsec),
              'F' => {
                    try!(parse_type(fmt, 'Y', tm));
                    try!('-'.fmt(fmt));
                    try!(parse_type(fmt, 'm', tm));
                    try!('-'.fmt(fmt));
                    return parse_type(fmt, 'd', tm);
              }
              'G' => return iso_week(fmt, 'G', tm),
              'g' => return iso_week(fmt, 'g', tm),
              'H' => return write!(fmt, "{:02d}", tm.tm_hour),
              'I' => {
                let mut h = tm.tm_hour;
                if h == 0 { h = 12 }
                if h > 12 { h -= 12 }
                return write!(fmt, "{:02d}", h)
              }
              'j' => return write!(fmt, "{:03d}", tm.tm_yday + 1),
              'k' => return write!(fmt, "{:2d}", tm.tm_hour),
              'l' => {
                let mut h = tm.tm_hour;
                if h == 0 { h = 12 }
                if h > 12 { h -= 12 }
                return write!(fmt, "{:2d}", h)
              }
              'M' => return write!(fmt, "{:02d}", tm.tm_min),
              'm' => return write!(fmt, "{:02d}", tm.tm_mon + 1),
              'n' => "\n",
              'P' => if (tm.tm_hour as int) < 12 { "am" } else { "pm" },
              'p' => if (tm.tm_hour as int) < 12 { "AM" } else { "PM" },
              'R' => {
                    try!(parse_type(fmt, 'H', tm));
                    try!(':'.fmt(fmt));
                    return parse_type(fmt, 'M', tm);
              }
              'r' => {
                    try!(parse_type(fmt, 'I', tm));
                    try!(':'.fmt(fmt));
                    try!(parse_type(fmt, 'M', tm));
                    try!(':'.fmt(fmt));
                    try!(parse_type(fmt, 'S', tm));
                    try!(' '.fmt(fmt));
                    return parse_type(fmt, 'p', tm);
              }
              'S' => return write!(fmt, "{:02d}", tm.tm_sec),
              's' => return write!(fmt, "{}", tm.to_timespec().sec),
              'T' | 'X' => {
                    try!(parse_type(fmt, 'H', tm));
                    try!(':'.fmt(fmt));
                    try!(parse_type(fmt, 'M', tm));
                    try!(':'.fmt(fmt));
                    return parse_type(fmt, 'S', tm);
              }
              't' => "\t",
              'U' => return write!(fmt, "{:02d}", (tm.tm_yday - tm.tm_wday + 7) / 7),
              'u' => {
                let i = tm.tm_wday as int;
                return (if i == 0 { 7 } else { i }).fmt(fmt);
              }
              'V' => return iso_week(fmt, 'V', tm),
              'v' => {
                  try!(parse_type(fmt, 'e', tm));
                  try!('-'.fmt(fmt));
                  try!(parse_type(fmt, 'b', tm));
                  try!('-'.fmt(fmt));
                  return parse_type(fmt, 'Y', tm);
              }
              'W' => {
                  return write!(fmt, "{:02d}",
                                 (tm.tm_yday - (tm.tm_wday - 1 + 7) % 7 + 7) / 7)
              }
              'w' => return (tm.tm_wday as int).fmt(fmt),
              'Y' => return (tm.tm_year as int + 1900).fmt(fmt),
              'y' => return write!(fmt, "{:02d}", (tm.tm_year as int + 1900) % 100),
              'Z' => if tm.tm_gmtoff == 0_i32 { "GMT"} else { "" }, // FIXME (#2350): support locale
              'z' => {
                let sign = if tm.tm_gmtoff > 0_i32 { '+' } else { '-' };
                let mut m = num::abs(tm.tm_gmtoff) / 60_i32;
                let h = m / 60_i32;
                m -= h * 60_i32;
                return write!(fmt, "{}{:02d}{:02d}", sign, h, m);
              }
              '+' => return tm.rfc3339().fmt(fmt),
              '%' => "%",
              _   => return die()
            }.fmt(fmt)
        }

        match self.format {
            FmtStr(ref s) => {
                let mut chars = s.chars();
                loop {
                    match chars.next() {
                        Some('%') => {
                            // we've already validated that % always precedes another char
                            try!(parse_type(fmt, chars.next().unwrap(), self.tm));
                        }
                        Some(ch) => try!(ch.fmt(fmt)),
                        None => break,
                    }
                }

                Ok(())
            }
            FmtCtime => {
                self.tm.to_local().asctime().fmt(fmt)
            }
            FmtRfc3339 => {
                if self.tm.tm_gmtoff == 0_i32 {
                    TmFmt {
                        tm: self.tm,
                        format: FmtStr("%Y-%m-%dT%H:%M:%SZ"),
                    }.fmt(fmt)
                } else {
                    let s = TmFmt {
                        tm: self.tm,
                        format: FmtStr("%Y-%m-%dT%H:%M:%S"),
                    };
                    let sign = if self.tm.tm_gmtoff > 0_i32 { '+' } else { '-' };
                    let mut m = num::abs(self.tm.tm_gmtoff) / 60_i32;
                    let h = m / 60_i32;
                    m -= h * 60_i32;
                    write!(fmt, "{}{}{:02d}:{:02d}", s, sign, h as int, m as int)
                }
            }
        }
    }
}

/// Parses the time from the string according to the format string.
pub fn strptime(s: &str, format: &str) -> Result<Tm, ParseError> {
    fn match_str(s: &str, pos: uint, needle: &str) -> bool {
        s.slice_from(pos).starts_with(needle)
    }

    fn match_strs(ss: &str, pos: uint, strs: &[(&str, i32)])
      -> Option<(i32, uint)> {
        for &(needle, value) in strs.iter() {
            if match_str(ss, pos, needle) {
                return Some((value, pos + needle.len()));
            }
        }

        None
    }

    fn match_digits(ss: &str, pos: uint, digits: uint, ws: bool)
      -> Option<(i32, uint)> {
        let mut pos = pos;
        let len = ss.len();
        let mut value = 0_i32;

        let mut i = 0u;
        while i < digits {
            if pos >= len {
                return None;
            }
            let range = ss.char_range_at(pos);
            pos = range.next;

            match range.ch {
              '0' ... '9' => {
                value = value * 10_i32 + (range.ch as i32 - '0' as i32);
              }
              ' ' if ws => (),
              _ => return None
            }
            i += 1u;
        }

        Some((value, pos))
    }

    fn match_fractional_seconds(ss: &str, pos: uint) -> (i32, uint) {
        let len = ss.len();
        let mut value = 0_i32;
        let mut multiplier = NSEC_PER_SEC / 10;
        let mut pos = pos;

        loop {
            if pos >= len {
                break;
            }
            let range = ss.char_range_at(pos);

            match range.ch {
                '0' ... '9' => {
                    pos = range.next;
                    // This will drop digits after the nanoseconds place
                    let digit = range.ch as i32 - '0' as i32;
                    value += digit * multiplier;
                    multiplier /= 10;
                }
                _ => break
            }
        }

        (value, pos)
    }

    fn match_digits_in_range(ss: &str, pos: uint, digits: uint, ws: bool,
                             min: i32, max: i32) -> Option<(i32, uint)> {
        match match_digits(ss, pos, digits, ws) {
          Some((val, pos)) if val >= min && val <= max => {
            Some((val, pos))
          }
          _ => None
        }
    }

    fn parse_char(s: &str, pos: uint, c: char) -> Result<uint, ParseError> {
        let range = s.char_range_at(pos);

        if c == range.ch {
            Ok(range.next)
        } else {
            Err(UnexpectedCharacter(c, range.ch))
        }
    }

    fn parse_type(s: &str, pos: uint, ch: char, tm: &mut Tm)
      -> Result<uint, ParseError> {
        match ch {
          'A' => match match_strs(s, pos, [
              ("Sunday", 0_i32),
              ("Monday", 1_i32),
              ("Tuesday", 2_i32),
              ("Wednesday", 3_i32),
              ("Thursday", 4_i32),
              ("Friday", 5_i32),
              ("Saturday", 6_i32)
          ]) {
            Some(item) => { let (v, pos) = item; tm.tm_wday = v; Ok(pos) }
            None => Err(InvalidDay)
          },
          'a' => match match_strs(s, pos, [
              ("Sun", 0_i32),
              ("Mon", 1_i32),
              ("Tue", 2_i32),
              ("Wed", 3_i32),
              ("Thu", 4_i32),
              ("Fri", 5_i32),
              ("Sat", 6_i32)
          ]) {
            Some(item) => { let (v, pos) = item; tm.tm_wday = v; Ok(pos) }
            None => Err(InvalidDay)
          },
          'B' => match match_strs(s, pos, [
              ("January", 0_i32),
              ("February", 1_i32),
              ("March", 2_i32),
              ("April", 3_i32),
              ("May", 4_i32),
              ("June", 5_i32),
              ("July", 6_i32),
              ("August", 7_i32),
              ("September", 8_i32),
              ("October", 9_i32),
              ("November", 10_i32),
              ("December", 11_i32)
          ]) {
            Some(item) => { let (v, pos) = item; tm.tm_mon = v; Ok(pos) }
            None => Err(InvalidMonth)
          },
          'b' | 'h' => match match_strs(s, pos, [
              ("Jan", 0_i32),
              ("Feb", 1_i32),
              ("Mar", 2_i32),
              ("Apr", 3_i32),
              ("May", 4_i32),
              ("Jun", 5_i32),
              ("Jul", 6_i32),
              ("Aug", 7_i32),
              ("Sep", 8_i32),
              ("Oct", 9_i32),
              ("Nov", 10_i32),
              ("Dec", 11_i32)
          ]) {
            Some(item) => { let (v, pos) = item; tm.tm_mon = v; Ok(pos) }
            None => Err(InvalidMonth)
          },
          'C' => match match_digits_in_range(s, pos, 2u, false, 0_i32,
                                             99_i32) {
            Some(item) => {
                let (v, pos) = item;
                  tm.tm_year += (v * 100_i32) - 1900_i32;
                  Ok(pos)
              }
            None => Err(InvalidYear)
          },
          'c' => {
            parse_type(s, pos, 'a', &mut *tm)
                .and_then(|pos| parse_char(s, pos, ' '))
                .and_then(|pos| parse_type(s, pos, 'b', &mut *tm))
                .and_then(|pos| parse_char(s, pos, ' '))
                .and_then(|pos| parse_type(s, pos, 'e', &mut *tm))
                .and_then(|pos| parse_char(s, pos, ' '))
                .and_then(|pos| parse_type(s, pos, 'T', &mut *tm))
                .and_then(|pos| parse_char(s, pos, ' '))
                .and_then(|pos| parse_type(s, pos, 'Y', &mut *tm))
          }
          'D' | 'x' => {
            parse_type(s, pos, 'm', &mut *tm)
                .and_then(|pos| parse_char(s, pos, '/'))
                .and_then(|pos| parse_type(s, pos, 'd', &mut *tm))
                .and_then(|pos| parse_char(s, pos, '/'))
                .and_then(|pos| parse_type(s, pos, 'y', &mut *tm))
          }
          'd' => match match_digits_in_range(s, pos, 2u, false, 1_i32,
                                             31_i32) {
            Some(item) => { let (v, pos) = item; tm.tm_mday = v; Ok(pos) }
            None => Err(InvalidDayOfMonth)
          },
          'e' => match match_digits_in_range(s, pos, 2u, true, 1_i32,
                                             31_i32) {
            Some(item) => { let (v, pos) = item; tm.tm_mday = v; Ok(pos) }
            None => Err(InvalidDayOfMonth)
          },
          'f' => {
            let (val, pos) = match_fractional_seconds(s, pos);
            tm.tm_nsec = val;
            Ok(pos)
          }
          'F' => {
            parse_type(s, pos, 'Y', &mut *tm)
                .and_then(|pos| parse_char(s, pos, '-'))
                .and_then(|pos| parse_type(s, pos, 'm', &mut *tm))
                .and_then(|pos| parse_char(s, pos, '-'))
                .and_then(|pos| parse_type(s, pos, 'd', &mut *tm))
          }
          'H' => {
            match match_digits_in_range(s, pos, 2u, false, 0_i32, 23_i32) {
              Some(item) => { let (v, pos) = item; tm.tm_hour = v; Ok(pos) }
              None => Err(InvalidHour)
            }
          }
          'I' => {
            match match_digits_in_range(s, pos, 2u, false, 1_i32, 12_i32) {
              Some(item) => {
                  let (v, pos) = item;
                  tm.tm_hour = if v == 12_i32 { 0_i32 } else { v };
                  Ok(pos)
              }
              None => Err(InvalidHour)
            }
          }
          'j' => {
            match match_digits_in_range(s, pos, 3u, false, 1_i32, 366_i32) {
              Some(item) => {
                let (v, pos) = item;
                tm.tm_yday = v - 1_i32;
                Ok(pos)
              }
              None => Err(InvalidDayOfYear)
            }
          }
          'k' => {
            match match_digits_in_range(s, pos, 2u, true, 0_i32, 23_i32) {
              Some(item) => { let (v, pos) = item; tm.tm_hour = v; Ok(pos) }
              None => Err(InvalidHour)
            }
          }
          'l' => {
            match match_digits_in_range(s, pos, 2u, true, 1_i32, 12_i32) {
              Some(item) => {
                  let (v, pos) = item;
                  tm.tm_hour = if v == 12_i32 { 0_i32 } else { v };
                  Ok(pos)
              }
              None => Err(InvalidHour)
            }
          }
          'M' => {
            match match_digits_in_range(s, pos, 2u, false, 0_i32, 59_i32) {
              Some(item) => { let (v, pos) = item; tm.tm_min = v; Ok(pos) }
              None => Err(InvalidMinute)
            }
          }
          'm' => {
            match match_digits_in_range(s, pos, 2u, false, 1_i32, 12_i32) {
              Some(item) => {
                let (v, pos) = item;
                tm.tm_mon = v - 1_i32;
                Ok(pos)
              }
              None => Err(InvalidMonth)
            }
          }
          'n' => parse_char(s, pos, '\n'),
          'P' => match match_strs(s, pos,
                                  [("am", 0_i32), ("pm", 12_i32)]) {

            Some(item) => { let (v, pos) = item; tm.tm_hour += v; Ok(pos) }
            None => Err(InvalidHour)
          },
          'p' => match match_strs(s, pos,
                                  [("AM", 0_i32), ("PM", 12_i32)]) {

            Some(item) => { let (v, pos) = item; tm.tm_hour += v; Ok(pos) }
            None => Err(InvalidHour)
          },
          'R' => {
            parse_type(s, pos, 'H', &mut *tm)
                .and_then(|pos| parse_char(s, pos, ':'))
                .and_then(|pos| parse_type(s, pos, 'M', &mut *tm))
          }
          'r' => {
            parse_type(s, pos, 'I', &mut *tm)
                .and_then(|pos| parse_char(s, pos, ':'))
                .and_then(|pos| parse_type(s, pos, 'M', &mut *tm))
                .and_then(|pos| parse_char(s, pos, ':'))
                .and_then(|pos| parse_type(s, pos, 'S', &mut *tm))
                .and_then(|pos| parse_char(s, pos, ' '))
                .and_then(|pos| parse_type(s, pos, 'p', &mut *tm))
          }
          'S' => {
            match match_digits_in_range(s, pos, 2u, false, 0_i32, 60_i32) {
              Some(item) => {
                let (v, pos) = item;
                tm.tm_sec = v;
                Ok(pos)
              }
              None => Err(InvalidSecond)
            }
          }
          //'s' {}
          'T' | 'X' => {
            parse_type(s, pos, 'H', &mut *tm)
                .and_then(|pos| parse_char(s, pos, ':'))
                .and_then(|pos| parse_type(s, pos, 'M', &mut *tm))
                .and_then(|pos| parse_char(s, pos, ':'))
                .and_then(|pos| parse_type(s, pos, 'S', &mut *tm))
          }
          't' => parse_char(s, pos, '\t'),
          'u' => {
            match match_digits_in_range(s, pos, 1u, false, 1_i32, 7_i32) {
              Some(item) => {
                let (v, pos) = item;
                tm.tm_wday = if v == 7 { 0 } else { v };
                Ok(pos)
              }
              None => Err(InvalidDayOfWeek)
            }
          }
          'v' => {
            parse_type(s, pos, 'e', &mut *tm)
                .and_then(|pos|  parse_char(s, pos, '-'))
                .and_then(|pos| parse_type(s, pos, 'b', &mut *tm))
                .and_then(|pos| parse_char(s, pos, '-'))
                .and_then(|pos| parse_type(s, pos, 'Y', &mut *tm))
          }
          //'W' {}
          'w' => {
            match match_digits_in_range(s, pos, 1u, false, 0_i32, 6_i32) {
              Some(item) => { let (v, pos) = item; tm.tm_wday = v; Ok(pos) }
              None => Err(InvalidDayOfWeek)
            }
          }
          'Y' => {
            match match_digits(s, pos, 4u, false) {
              Some(item) => {
                let (v, pos) = item;
                tm.tm_year = v - 1900_i32;
                Ok(pos)
              }
              None => Err(InvalidYear)
            }
          }
          'y' => {
            match match_digits_in_range(s, pos, 2u, false, 0_i32, 99_i32) {
              Some(item) => {
                let (v, pos) = item;
                tm.tm_year = v;
                Ok(pos)
              }
              None => Err(InvalidYear)
            }
          }
          'Z' => {
            if match_str(s, pos, "UTC") || match_str(s, pos, "GMT") {
                tm.tm_gmtoff = 0_i32;
                Ok(pos + 3u)
            } else {
                // It's odd, but to maintain compatibility with c's
                // strptime we ignore the timezone.
                let mut pos = pos;
                let len = s.len();
                while pos < len {
                    let range = s.char_range_at(pos);
                    pos = range.next;
                    if range.ch == ' ' { break; }
                }

                Ok(pos)
            }
          }
          'z' => {
            let range = s.char_range_at(pos);

            if range.ch == '+' || range.ch == '-' {
                match match_digits(s, range.next, 4u, false) {
                  Some(item) => {
                    let (v, pos) = item;
                    if v == 0_i32 {
                        tm.tm_gmtoff = 0_i32;
                    }

                    Ok(pos)
                  }
                  None => Err(InvalidZoneOffset)
                }
            } else {
                Err(InvalidZoneOffset)
            }
          }
          '%' => parse_char(s, pos, '%'),
          ch => Err(InvalidFormatSpecifier(ch))
        }
    }

    let mut rdr = BufReader::new(format.as_bytes());
    let mut tm = Tm {
        tm_sec: 0_i32,
        tm_min: 0_i32,
        tm_hour: 0_i32,
        tm_mday: 0_i32,
        tm_mon: 0_i32,
        tm_year: 0_i32,
        tm_wday: 0_i32,
        tm_yday: 0_i32,
        tm_isdst: 0_i32,
        tm_gmtoff: 0_i32,
        tm_nsec: 0_i32,
    };
    let mut pos = 0u;
    let len = s.len();
    let mut result = Err(InvalidTime);

    while pos < len {
        let range = s.char_range_at(pos);
        let ch = range.ch;
        let next = range.next;

        let mut buf = [0];
        let c = match rdr.read(buf) {
            Ok(..) => buf[0] as char,
            Err(..) => break
        };
        match c {
            '%' => {
                let ch = match rdr.read(buf) {
                    Ok(..) => buf[0] as char,
                    Err(..) => break
                };
                match parse_type(s, pos, ch, &mut tm) {
                    Ok(next) => pos = next,
                    Err(e) => { result = Err(e); break; }
                }
            },
            c => {
                if c != ch { break }
                pos = next;
            }
        }
    }

    if pos == len && rdr.tell().unwrap() == format.len() as u64 {
        Ok(Tm {
            tm_sec: tm.tm_sec,
            tm_min: tm.tm_min,
            tm_hour: tm.tm_hour,
            tm_mday: tm.tm_mday,
            tm_mon: tm.tm_mon,
            tm_year: tm.tm_year,
            tm_wday: tm.tm_wday,
            tm_yday: tm.tm_yday,
            tm_isdst: tm.tm_isdst,
            tm_gmtoff: tm.tm_gmtoff,
            tm_nsec: tm.tm_nsec,
        })
    } else { result }
}

/// Formats the time according to the format string.
pub fn strftime(format: &str, tm: &Tm) -> Result<String, ParseError> {
    tm.strftime(format).map(|fmt| fmt.to_string())
}

#[cfg(test)]
mod tests {
    extern crate test;
    use super::{Timespec, InvalidTime, InvalidYear, get_time, precise_time_ns,
                precise_time_s, tzset, at_utc, at, strptime, MissingFormatConverter,
                InvalidFormatSpecifier};

    use std::f64;
    use std::result::{Err, Ok};
    use std::time::Duration;
    use self::test::Bencher;

    #[cfg(windows)]
    fn set_time_zone() {
        use libc;
        // Windows crt doesn't see any environment variable set by
        // `SetEnvironmentVariable`, which `os::setenv` internally uses.
        // It is why we use `putenv` here.
        extern {
            fn _putenv(envstring: *const libc::c_char) -> libc::c_int;
        }

        unsafe {
            // Windows does not understand "America/Los_Angeles".
            // PST+08 may look wrong, but not! "PST" indicates
            // the name of timezone. "+08" means UTC = local + 08.
            "TZ=PST+08".with_c_str(|env| {
                _putenv(env);
            })
        }
        tzset();
    }
    #[cfg(not(windows))]
    fn set_time_zone() {
        use std::os;
        os::setenv("TZ", "America/Los_Angeles");
        tzset();
    }

    fn test_get_time() {
        static SOME_RECENT_DATE: i64 = 1325376000i64; // 2012-01-01T00:00:00Z
        static SOME_FUTURE_DATE: i64 = 1577836800i64; // 2020-01-01T00:00:00Z

        let tv1 = get_time();
        debug!("tv1={} sec + {} nsec", tv1.sec as uint, tv1.nsec as uint);

        assert!(tv1.sec > SOME_RECENT_DATE);
        assert!(tv1.nsec < 1000000000i32);

        let tv2 = get_time();
        debug!("tv2={} sec + {} nsec", tv2.sec as uint, tv2.nsec as uint);

        assert!(tv2.sec >= tv1.sec);
        assert!(tv2.sec < SOME_FUTURE_DATE);
        assert!(tv2.nsec < 1000000000i32);
        if tv2.sec == tv1.sec {
            assert!(tv2.nsec >= tv1.nsec);
        }
    }

    fn test_precise_time() {
        let s0 = precise_time_s();
        debug!("s0={} sec", f64::to_str_digits(s0, 9u));
        assert!(s0 > 0.);

        let ns0 = precise_time_ns();
        let ns1 = precise_time_ns();
        debug!("ns0={} ns", ns0);
        debug!("ns1={} ns", ns1);
        assert!(ns1 >= ns0);

        let ns2 = precise_time_ns();
        debug!("ns2={} ns", ns2);
        assert!(ns2 >= ns1);
    }

    fn test_at_utc() {
        set_time_zone();

        let time = Timespec::new(1234567890, 54321);
        let utc = at_utc(time);

        assert_eq!(utc.tm_sec, 30_i32);
        assert_eq!(utc.tm_min, 31_i32);
        assert_eq!(utc.tm_hour, 23_i32);
        assert_eq!(utc.tm_mday, 13_i32);
        assert_eq!(utc.tm_mon, 1_i32);
        assert_eq!(utc.tm_year, 109_i32);
        assert_eq!(utc.tm_wday, 5_i32);
        assert_eq!(utc.tm_yday, 43_i32);
        assert_eq!(utc.tm_isdst, 0_i32);
        assert_eq!(utc.tm_gmtoff, 0_i32);
        assert_eq!(utc.tm_nsec, 54321_i32);
    }

    fn test_at() {
        set_time_zone();

        let time = Timespec::new(1234567890, 54321);
        let local = at(time);

        debug!("time_at: {}", local);

        assert_eq!(local.tm_sec, 30_i32);
        assert_eq!(local.tm_min, 31_i32);
        assert_eq!(local.tm_hour, 15_i32);
        assert_eq!(local.tm_mday, 13_i32);
        assert_eq!(local.tm_mon, 1_i32);
        assert_eq!(local.tm_year, 109_i32);
        assert_eq!(local.tm_wday, 5_i32);
        assert_eq!(local.tm_yday, 43_i32);
        assert_eq!(local.tm_isdst, 0_i32);
        assert_eq!(local.tm_gmtoff, -28800_i32);
        assert_eq!(local.tm_nsec, 54321_i32);
    }

    fn test_to_timespec() {
        set_time_zone();

        let time = Timespec::new(1234567890, 54321);
        let utc = at_utc(time);

        assert_eq!(utc.to_timespec(), time);
        assert_eq!(utc.to_local().to_timespec(), time);
    }

    fn test_conversions() {
        set_time_zone();

        let time = Timespec::new(1234567890, 54321);
        let utc = at_utc(time);
        let local = at(time);

        assert!(local.to_local() == local);
        assert!(local.to_utc() == utc);
        assert!(local.to_utc().to_local() == local);
        assert!(utc.to_utc() == utc);
        assert!(utc.to_local() == local);
        assert!(utc.to_local().to_utc() == utc);
    }

    fn test_strptime() {
        set_time_zone();

        match strptime("", "") {
          Ok(ref tm) => {
            assert!(tm.tm_sec == 0_i32);
            assert!(tm.tm_min == 0_i32);
            assert!(tm.tm_hour == 0_i32);
            assert!(tm.tm_mday == 0_i32);
            assert!(tm.tm_mon == 0_i32);
            assert!(tm.tm_year == 0_i32);
            assert!(tm.tm_wday == 0_i32);
            assert!(tm.tm_isdst == 0_i32);
            assert!(tm.tm_gmtoff == 0_i32);
            assert!(tm.tm_nsec == 0_i32);
          }
          Err(_) => ()
        }

        let format = "%a %b %e %T.%f %Y";
        assert_eq!(strptime("", format), Err(InvalidTime));
        assert!(strptime("Fri Feb 13 15:31:30", format)
            == Err(InvalidTime));

        match strptime("Fri Feb 13 15:31:30.01234 2009", format) {
          Err(e) => panic!(e),
          Ok(ref tm) => {
            assert!(tm.tm_sec == 30_i32);
            assert!(tm.tm_min == 31_i32);
            assert!(tm.tm_hour == 15_i32);
            assert!(tm.tm_mday == 13_i32);
            assert!(tm.tm_mon == 1_i32);
            assert!(tm.tm_year == 109_i32);
            assert!(tm.tm_wday == 5_i32);
            assert!(tm.tm_yday == 0_i32);
            assert!(tm.tm_isdst == 0_i32);
            assert!(tm.tm_gmtoff == 0_i32);
            assert!(tm.tm_nsec == 12340000_i32);
          }
        }

        fn test(s: &str, format: &str) -> bool {
            match strptime(s, format) {
              Ok(ref tm) => {
                tm.strftime(format).unwrap().to_string() == s.to_string()
              },
              Err(e) => panic!(e)
            }
        }

        let days = [
            "Sunday".to_string(),
            "Monday".to_string(),
            "Tuesday".to_string(),
            "Wednesday".to_string(),
            "Thursday".to_string(),
            "Friday".to_string(),
            "Saturday".to_string()
        ];
        for day in days.iter() {
            assert!(test(day.as_slice(), "%A"));
        }

        let days = [
            "Sun".to_string(),
            "Mon".to_string(),
            "Tue".to_string(),
            "Wed".to_string(),
            "Thu".to_string(),
            "Fri".to_string(),
            "Sat".to_string()
        ];
        for day in days.iter() {
            assert!(test(day.as_slice(), "%a"));
        }

        let months = [
            "January".to_string(),
            "February".to_string(),
            "March".to_string(),
            "April".to_string(),
            "May".to_string(),
            "June".to_string(),
            "July".to_string(),
            "August".to_string(),
            "September".to_string(),
            "October".to_string(),
            "November".to_string(),
            "December".to_string()
        ];
        for day in months.iter() {
            assert!(test(day.as_slice(), "%B"));
        }

        let months = [
            "Jan".to_string(),
            "Feb".to_string(),
            "Mar".to_string(),
            "Apr".to_string(),
            "May".to_string(),
            "Jun".to_string(),
            "Jul".to_string(),
            "Aug".to_string(),
            "Sep".to_string(),
            "Oct".to_string(),
            "Nov".to_string(),
            "Dec".to_string()
        ];
        for day in months.iter() {
            assert!(test(day.as_slice(), "%b"));
        }

        assert!(test("19", "%C"));
        assert!(test("Fri Feb 13 23:31:30 2009", "%c"));
        assert!(test("02/13/09", "%D"));
        assert!(test("03", "%d"));
        assert!(test("13", "%d"));
        assert!(test(" 3", "%e"));
        assert!(test("13", "%e"));
        assert!(test("2009-02-13", "%F"));
        assert!(test("03", "%H"));
        assert!(test("13", "%H"));
        assert!(test("03", "%I")); // FIXME (#2350): flesh out
        assert!(test("11", "%I")); // FIXME (#2350): flesh out
        assert!(test("044", "%j"));
        assert!(test(" 3", "%k"));
        assert!(test("13", "%k"));
        assert!(test(" 1", "%l"));
        assert!(test("11", "%l"));
        assert!(test("03", "%M"));
        assert!(test("13", "%M"));
        assert!(test("\n", "%n"));
        assert!(test("am", "%P"));
        assert!(test("pm", "%P"));
        assert!(test("AM", "%p"));
        assert!(test("PM", "%p"));
        assert!(test("23:31", "%R"));
        assert!(test("11:31:30 AM", "%r"));
        assert!(test("11:31:30 PM", "%r"));
        assert!(test("03", "%S"));
        assert!(test("13", "%S"));
        assert!(test("15:31:30", "%T"));
        assert!(test("\t", "%t"));
        assert!(test("1", "%u"));
        assert!(test("7", "%u"));
        assert!(test("13-Feb-2009", "%v"));
        assert!(test("0", "%w"));
        assert!(test("6", "%w"));
        assert!(test("2009", "%Y"));
        assert!(test("09", "%y"));
        assert!(strptime("-0000", "%z").unwrap().tm_gmtoff ==
            0);
        assert!(strptime("-0800", "%z").unwrap().tm_gmtoff ==
            0);
        assert!(test("%", "%%"));

        // Test for #7256
        assert_eq!(strptime("360", "%Y-%m-%d"), Err(InvalidYear));
    }

    fn test_asctime() {
        set_time_zone();

        let time = Timespec::new(1234567890, 54321);
        let utc   = at_utc(time);
        let local = at(time);

        debug!("test_ctime: {} {}", utc.asctime(), local.asctime());

        assert_eq!(utc.asctime().to_string(), "Fri Feb 13 23:31:30 2009".to_string());
        assert_eq!(local.asctime().to_string(), "Fri Feb 13 15:31:30 2009".to_string());
    }

    fn test_ctime() {
        set_time_zone();

        let time = Timespec::new(1234567890, 54321);
        let utc   = at_utc(time);
        let local = at(time);

        debug!("test_ctime: {} {}", utc.ctime(), local.ctime());

        assert_eq!(utc.ctime().to_string(), "Fri Feb 13 15:31:30 2009".to_string());
        assert_eq!(local.ctime().to_string(), "Fri Feb 13 15:31:30 2009".to_string());
    }

    fn test_strftime() {
        set_time_zone();

        let time = Timespec::new(1234567890, 54321);
        let utc = at_utc(time);
        let local = at(time);

        assert_eq!(local.strftime("").unwrap().to_string(), "".to_string());
        assert_eq!(local.strftime("%A").unwrap().to_string(), "Friday".to_string());
        assert_eq!(local.strftime("%a").unwrap().to_string(), "Fri".to_string());
        assert_eq!(local.strftime("%B").unwrap().to_string(), "February".to_string());
        assert_eq!(local.strftime("%b").unwrap().to_string(), "Feb".to_string());
        assert_eq!(local.strftime("%C").unwrap().to_string(), "20".to_string());
        assert_eq!(local.strftime("%c").unwrap().to_string(),
                   "Fri Feb 13 15:31:30 2009".to_string());
        assert_eq!(local.strftime("%D").unwrap().to_string(), "02/13/09".to_string());
        assert_eq!(local.strftime("%d").unwrap().to_string(), "13".to_string());
        assert_eq!(local.strftime("%e").unwrap().to_string(), "13".to_string());
        assert_eq!(local.strftime("%F").unwrap().to_string(), "2009-02-13".to_string());
        assert_eq!(local.strftime("%f").unwrap().to_string(), "000054321".to_string());
        assert_eq!(local.strftime("%G").unwrap().to_string(), "2009".to_string());
        assert_eq!(local.strftime("%g").unwrap().to_string(), "09".to_string());
        assert_eq!(local.strftime("%H").unwrap().to_string(), "15".to_string());
        assert_eq!(local.strftime("%h").unwrap().to_string(), "Feb".to_string());
        assert_eq!(local.strftime("%I").unwrap().to_string(), "03".to_string());
        assert_eq!(local.strftime("%j").unwrap().to_string(), "044".to_string());
        assert_eq!(local.strftime("%k").unwrap().to_string(), "15".to_string());
        assert_eq!(local.strftime("%l").unwrap().to_string(), " 3".to_string());
        assert_eq!(local.strftime("%M").unwrap().to_string(), "31".to_string());
        assert_eq!(local.strftime("%m").unwrap().to_string(), "02".to_string());
        assert_eq!(local.strftime("%n").unwrap().to_string(), "\n".to_string());
        assert_eq!(local.strftime("%P").unwrap().to_string(), "pm".to_string());
        assert_eq!(local.strftime("%p").unwrap().to_string(), "PM".to_string());
        assert_eq!(local.strftime("%R").unwrap().to_string(), "15:31".to_string());
        assert_eq!(local.strftime("%r").unwrap().to_string(), "03:31:30 PM".to_string());
        assert_eq!(local.strftime("%S").unwrap().to_string(), "30".to_string());
        assert_eq!(local.strftime("%s").unwrap().to_string(), "1234567890".to_string());
        assert_eq!(local.strftime("%T").unwrap().to_string(), "15:31:30".to_string());
        assert_eq!(local.strftime("%t").unwrap().to_string(), "\t".to_string());
        assert_eq!(local.strftime("%U").unwrap().to_string(), "06".to_string());
        assert_eq!(local.strftime("%u").unwrap().to_string(), "5".to_string());
        assert_eq!(local.strftime("%V").unwrap().to_string(), "07".to_string());
        assert_eq!(local.strftime("%v").unwrap().to_string(), "13-Feb-2009".to_string());
        assert_eq!(local.strftime("%W").unwrap().to_string(), "06".to_string());
        assert_eq!(local.strftime("%w").unwrap().to_string(), "5".to_string());
        // FIXME (#2350): support locale
        assert_eq!(local.strftime("%X").unwrap().to_string(), "15:31:30".to_string());
        // FIXME (#2350): support locale
        assert_eq!(local.strftime("%x").unwrap().to_string(), "02/13/09".to_string());
        assert_eq!(local.strftime("%Y").unwrap().to_string(), "2009".to_string());
        assert_eq!(local.strftime("%y").unwrap().to_string(), "09".to_string());
        // FIXME (#2350): support locale
        assert_eq!(local.strftime("%Z").unwrap().to_string(), "".to_string());
        assert_eq!(local.strftime("%z").unwrap().to_string(), "-0800".to_string());
        assert_eq!(local.strftime("%+").unwrap().to_string(),
                   "2009-02-13T15:31:30-08:00".to_string());
        assert_eq!(local.strftime("%%").unwrap().to_string(), "%".to_string());

         let invalid_specifiers = ["%E", "%J", "%K", "%L", "%N", "%O", "%o", "%Q", "%q"];
        for &sp in invalid_specifiers.iter() {
            assert_eq!(local.strftime(sp).unwrap_err(), InvalidFormatSpecifier(sp.char_at(1)));
        }
        assert_eq!(local.strftime("%").unwrap_err(), MissingFormatConverter);
        assert_eq!(local.strftime("%A %").unwrap_err(), MissingFormatConverter);

        assert_eq!(local.asctime().to_string(), "Fri Feb 13 15:31:30 2009".to_string());
        assert_eq!(local.ctime().to_string(), "Fri Feb 13 15:31:30 2009".to_string());
        assert_eq!(local.rfc822z().to_string(), "Fri, 13 Feb 2009 15:31:30 -0800".to_string());
        assert_eq!(local.rfc3339().to_string(), "2009-02-13T15:31:30-08:00".to_string());

        assert_eq!(utc.asctime().to_string(), "Fri Feb 13 23:31:30 2009".to_string());
        assert_eq!(utc.ctime().to_string(), "Fri Feb 13 15:31:30 2009".to_string());
        assert_eq!(utc.rfc822().to_string(), "Fri, 13 Feb 2009 23:31:30 GMT".to_string());
        assert_eq!(utc.rfc822z().to_string(), "Fri, 13 Feb 2009 23:31:30 -0000".to_string());
        assert_eq!(utc.rfc3339().to_string(), "2009-02-13T23:31:30Z".to_string());
    }

    fn test_timespec_eq_ord() {
        let a = &Timespec::new(-2, 1);
        let b = &Timespec::new(-1, 2);
        let c = &Timespec::new(1, 2);
        let d = &Timespec::new(2, 1);
        let e = &Timespec::new(2, 1);

        assert!(d.eq(e));
        assert!(c.ne(e));

        assert!(a.lt(b));
        assert!(b.lt(c));
        assert!(c.lt(d));

        assert!(a.le(b));
        assert!(b.le(c));
        assert!(c.le(d));
        assert!(d.le(e));
        assert!(e.le(d));

        assert!(b.ge(a));
        assert!(c.ge(b));
        assert!(d.ge(c));
        assert!(e.ge(d));
        assert!(d.ge(e));

        assert!(b.gt(a));
        assert!(c.gt(b));
        assert!(d.gt(c));
    }

    fn test_timespec_add() {
        let a = Timespec::new(1, 2);
        let b = Duration::seconds(2) + Duration::nanoseconds(3);
        let c = a + b;
        assert_eq!(c.sec, 3);
        assert_eq!(c.nsec, 5);

        let p = Timespec::new(1, super::NSEC_PER_SEC - 2);
        let q = Duration::seconds(2) + Duration::nanoseconds(2);
        let r = p + q;
        assert_eq!(r.sec, 4);
        assert_eq!(r.nsec, 0);

        let u = Timespec::new(1, super::NSEC_PER_SEC - 2);
        let v = Duration::seconds(2) + Duration::nanoseconds(3);
        let w = u + v;
        assert_eq!(w.sec, 4);
        assert_eq!(w.nsec, 1);

        let k = Timespec::new(1, 0);
        let l = Duration::nanoseconds(-1);
        let m = k + l;
        assert_eq!(m.sec, 0);
        assert_eq!(m.nsec, 999_999_999);
    }

    fn test_timespec_sub() {
        let a = Timespec::new(2, 3);
        let b = Timespec::new(1, 2);
        let c = a - b;
        assert_eq!(c.num_nanoseconds(), Some(super::NSEC_PER_SEC as i64 + 1));

        let p = Timespec::new(2, 0);
        let q = Timespec::new(1, 2);
        let r = p - q;
        assert_eq!(r.num_nanoseconds(), Some(super::NSEC_PER_SEC as i64 - 2));

        let u = Timespec::new(1, 2);
        let v = Timespec::new(2, 3);
        let w = u - v;
        assert_eq!(w.num_nanoseconds(), Some(-super::NSEC_PER_SEC as i64 - 1));
    }

    #[test]
    #[cfg_attr(target_os = "android", ignore)] // FIXME #10958
    fn run_tests() {
        // The tests race on tzset. So instead of having many independent
        // tests, we will just call the functions now.
        test_get_time();
        test_precise_time();
        test_at_utc();
        test_at();
        test_to_timespec();
        test_conversions();
        test_strptime();
        test_asctime();
        test_ctime();
        test_strftime();
        test_timespec_eq_ord();
        test_timespec_add();
        test_timespec_sub();
    }

    #[bench]
    fn bench_precise_time_ns(b: &mut Bencher) {
        b.iter(|| precise_time_ns())
    }
}
