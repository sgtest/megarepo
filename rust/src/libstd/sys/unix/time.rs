// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

pub use self::inner::SteadyTime;

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod inner {
    use libc;
    use time::Duration;
    use ops::Sub;
    use sync::{Once, ONCE_INIT};

    pub struct SteadyTime {
        t: u64
    }

    extern {
        pub fn mach_absolute_time() -> u64;
        pub fn mach_timebase_info(info: *mut libc::mach_timebase_info) -> libc::c_int;
    }

    impl SteadyTime {
        pub fn now() -> SteadyTime {
            SteadyTime {
                t: unsafe { mach_absolute_time() },
            }
        }

        pub fn ns(&self) -> u64 {
            let info = info();
            self.t * info.numer as u64 / info.denom as u64
        }
    }

    fn info() -> &'static libc::mach_timebase_info {
        static mut INFO: libc::mach_timebase_info = libc::mach_timebase_info {
            numer: 0,
            denom: 0,
        };
        static ONCE: Once = ONCE_INIT;

        unsafe {
            ONCE.call_once(|| {
                mach_timebase_info(&mut INFO);
            });
            &INFO
        }
    }

    impl<'a> Sub for &'a SteadyTime {
        type Output = Duration;

        fn sub(self, other: &SteadyTime) -> Duration {
            unsafe {
                let info = info();
                let diff = self.t as i64 - other.t as i64;
                Duration::nanoseconds(diff * info.numer as i64 / info.denom as i64)
            }
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
mod inner {
    use libc;
    use time::Duration;
    use ops::Sub;

    const NSEC_PER_SEC: i64 = 1_000_000_000;

    pub struct SteadyTime {
        t: libc::timespec,
    }

    // Apparently android provides this in some other library?
    // Bitrig's RT extensions are in the C library, not a separate librt
    // OpenBSD provide it via libc
    #[cfg(not(any(target_os = "android",
                  target_os = "bitrig",
                  target_os = "openbsd")))]
    #[link(name = "rt")]
    extern {}

    extern {
        fn clock_gettime(clk_id: libc::c_int, tp: *mut libc::timespec) -> libc::c_int;
    }

    impl SteadyTime {
        pub fn now() -> SteadyTime {
            let mut t = SteadyTime {
                t: libc::timespec {
                    tv_sec: 0,
                    tv_nsec: 0,
                }
            };
            unsafe {
                assert_eq!(0, clock_gettime(libc::CLOCK_MONOTONIC, &mut t.t));
            }
            t
        }

        pub fn ns(&self) -> u64 {
            self.t.tv_sec as u64 * NSEC_PER_SEC as u64 + self.t.tv_nsec as u64
        }
    }

    impl<'a> Sub for &'a SteadyTime {
        type Output = Duration;

        fn sub(self, other: &SteadyTime) -> Duration {
            if self.t.tv_nsec >= other.t.tv_nsec {
                Duration::seconds(self.t.tv_sec as i64 - other.t.tv_sec as i64) +
                    Duration::nanoseconds(self.t.tv_nsec as i64 - other.t.tv_nsec as i64)
            } else {
                Duration::seconds(self.t.tv_sec as i64 - 1 - other.t.tv_sec as i64) +
                    Duration::nanoseconds(self.t.tv_nsec as i64 + NSEC_PER_SEC -
                                          other.t.tv_nsec as i64)
            }
        }
    }
}
