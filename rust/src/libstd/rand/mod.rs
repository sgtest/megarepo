// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Utilities for random number generation
//!
//! The key functions are `random()` and `Rng::gen()`. These are polymorphic
//! and so can be used to generate any type that implements `Rand`. Type inference
//! means that often a simple call to `rand::random()` or `rng.gen()` will
//! suffice, but sometimes an annotation is required, e.g. `rand::random::<f64>()`.
//!
//! See the `distributions` submodule for sampling random numbers from
//! distributions like normal and exponential.
//!
//! # Task-local RNG
//!
//! There is built-in support for a RNG associated with each task stored
//! in task-local storage. This RNG can be accessed via `task_rng`, or
//! used implicitly via `random`. This RNG is normally randomly seeded
//! from an operating-system source of randomness, e.g. `/dev/urandom` on
//! Unix systems, and will automatically reseed itself from this source
//! after generating 32 KiB of random data.
//!
//! # Cryptographic security
//!
//! An application that requires an entropy source for cryptographic purposes
//! must use `OsRng`, which reads randomness from the source that the operating
//! system provides (e.g. `/dev/urandom` on Unixes or `CryptGenRandom()` on Windows).
//! The other random number generators provided by this module are not suitable
//! for such purposes.
//!
//! *Note*: many Unix systems provide `/dev/random` as well as `/dev/urandom`.
//! This module uses `/dev/urandom` for the following reasons:
//!
//! -   On Linux, `/dev/random` may block if entropy pool is empty; `/dev/urandom` will not block.
//!     This does not mean that `/dev/random` provides better output than
//!     `/dev/urandom`; the kernel internally runs a cryptographically secure pseudorandom
//!     number generator (CSPRNG) based on entropy pool for random number generation,
//!     so the "quality" of `/dev/random` is not better than `/dev/urandom` in most cases.
//!     However, this means that `/dev/urandom` can yield somewhat predictable randomness
//!     if the entropy pool is very small, such as immediately after first booting.
//!     Linux 3,17 added `getrandom(2)` system call which solves the issue: it blocks if entropy
//!     pool is not initialized yet, but it does not block once initialized.
//!     `OsRng` tries to use `getrandom(2)` if available, and use `/dev/urandom` fallback if not.
//!     If an application does not have `getrandom` and likely to be run soon after first booting,
//!     or on a system with very few entropy sources, one should consider using `/dev/random` via
//!     `ReaderRng`.
//! -   On some systems (e.g. FreeBSD, OpenBSD and Mac OS X) there is no difference
//!     between the two sources. (Also note that, on some systems e.g. FreeBSD, both `/dev/random`
//!     and `/dev/urandom` may block once if the CSPRNG has not seeded yet.)
//!
//! # Examples
//!
//! ```rust
//! use std::rand;
//! use std::rand::Rng;
//!
//! let mut rng = rand::task_rng();
//! if rng.gen() { // random bool
//!     println!("int: {}, uint: {}", rng.gen::<int>(), rng.gen::<uint>())
//! }
//! ```
//!
//! ```rust
//! use std::rand;
//!
//! let tuple = rand::random::<(f64, char)>();
//! println!("{}", tuple)
//! ```
//!
//! ## Monte Carlo estimation of π
//!
//! For this example, imagine we have a square with sides of length 2 and a unit
//! circle, both centered at the origin. Since the area of a unit circle is π,
//! we have:
//!
//! ```notrust
//!     (area of unit circle) / (area of square) = π / 4
//! ```
//!
//! So if we sample many points randomly from the square, roughly π / 4 of them
//! should be inside the circle.
//!
//! We can use the above fact to estimate the value of π: pick many points in the
//! square at random, calculate the fraction that fall within the circle, and
//! multiply this fraction by 4.
//!
//! ```
//! use std::rand;
//! use std::rand::distributions::{IndependentSample, Range};
//!
//! fn main() {
//!    let between = Range::new(-1f64, 1.);
//!    let mut rng = rand::task_rng();
//!
//!    let total = 1_000_000u;
//!    let mut in_circle = 0u;
//!
//!    for _ in range(0u, total) {
//!        let a = between.ind_sample(&mut rng);
//!        let b = between.ind_sample(&mut rng);
//!        if a*a + b*b <= 1. {
//!            in_circle += 1;
//!        }
//!    }
//!
//!    // prints something close to 3.14159...
//!    println!("{}", 4. * (in_circle as f64) / (total as f64));
//! }
//! ```
//!
//! ## Monty Hall Problem
//!
//! This is a simulation of the [Monty Hall Problem][]:
//!
//! > Suppose you're on a game show, and you're given the choice of three doors:
//! > Behind one door is a car; behind the others, goats. You pick a door, say No. 1,
//! > and the host, who knows what's behind the doors, opens another door, say No. 3,
//! > which has a goat. He then says to you, "Do you want to pick door No. 2?"
//! > Is it to your advantage to switch your choice?
//!
//! The rather unintuitive answer is that you will have a 2/3 chance of winning if
//! you switch and a 1/3 chance of winning of you don't, so it's better to switch.
//!
//! This program will simulate the game show and with large enough simulation steps
//! it will indeed confirm that it is better to switch.
//!
//! [Monty Hall Problem]: http://en.wikipedia.org/wiki/Monty_Hall_problem
//!
//! ```
//! use std::rand;
//! use std::rand::Rng;
//! use std::rand::distributions::{IndependentSample, Range};
//!
//! struct SimulationResult {
//!     win: bool,
//!     switch: bool,
//! }
//!
//! // Run a single simulation of the Monty Hall problem.
//! fn simulate<R: Rng>(random_door: &Range<uint>, rng: &mut R) -> SimulationResult {
//!     let car = random_door.ind_sample(rng);
//!
//!     // This is our initial choice
//!     let mut choice = random_door.ind_sample(rng);
//!
//!     // The game host opens a door
//!     let open = game_host_open(car, choice, rng);
//!
//!     // Shall we switch?
//!     let switch = rng.gen();
//!     if switch {
//!         choice = switch_door(choice, open);
//!     }
//!
//!     SimulationResult { win: choice == car, switch: switch }
//! }
//!
//! // Returns the door the game host opens given our choice and knowledge of
//! // where the car is. The game host will never open the door with the car.
//! fn game_host_open<R: Rng>(car: uint, choice: uint, rng: &mut R) -> uint {
//!     let choices = free_doors(&[car, choice]);
//!     rand::sample(rng, choices.into_iter(), 1)[0]
//! }
//!
//! // Returns the door we switch to, given our current choice and
//! // the open door. There will only be one valid door.
//! fn switch_door(choice: uint, open: uint) -> uint {
//!     free_doors(&[choice, open])[0]
//! }
//!
//! fn free_doors(blocked: &[uint]) -> Vec<uint> {
//!     range(0u, 3).filter(|x| !blocked.contains(x)).collect()
//! }
//!
//! fn main() {
//!     // The estimation will be more accurate with more simulations
//!     let num_simulations = 10000u;
//!
//!     let mut rng = rand::task_rng();
//!     let random_door = Range::new(0u, 3);
//!
//!     let (mut switch_wins, mut switch_losses) = (0u, 0u);
//!     let (mut keep_wins, mut keep_losses) = (0u, 0u);
//!
//!     println!("Running {} simulations...", num_simulations);
//!     for _ in range(0, num_simulations) {
//!         let result = simulate(&random_door, &mut rng);
//!
//!         match (result.win, result.switch) {
//!             (true, true) => switch_wins += 1,
//!             (true, false) => keep_wins += 1,
//!             (false, true) => switch_losses += 1,
//!             (false, false) => keep_losses += 1,
//!         }
//!     }
//!
//!     let total_switches = switch_wins + switch_losses;
//!     let total_keeps = keep_wins + keep_losses;
//!
//!     println!("Switched door {} times with {} wins and {} losses",
//!              total_switches, switch_wins, switch_losses);
//!
//!     println!("Kept our choice {} times with {} wins and {} losses",
//!              total_keeps, keep_wins, keep_losses);
//!
//!     // With a large number of simulations, the values should converge to
//!     // 0.667 and 0.333 respectively.
//!     println!("Estimated chance to win if we switch: {}",
//!              switch_wins as f32 / total_switches as f32);
//!     println!("Estimated chance to win if we don't: {}",
//!              keep_wins as f32 / total_keeps as f32);
//! }
//! ```

#![experimental]

use cell::RefCell;
use clone::Clone;
use io::IoResult;
use iter::{Iterator, IteratorExt};
use kinds::Copy;
use mem;
use rc::Rc;
use result::Result::{Ok, Err};
use vec::Vec;

#[cfg(not(target_word_size="64"))]
use core_rand::IsaacRng as IsaacWordRng;
#[cfg(target_word_size="64")]
use core_rand::Isaac64Rng as IsaacWordRng;

pub use core_rand::{Rand, Rng, SeedableRng, Open01, Closed01};
pub use core_rand::{XorShiftRng, IsaacRng, Isaac64Rng, ChaChaRng};
pub use core_rand::{distributions, reseeding};
pub use rand::os::OsRng;

pub mod os;
pub mod reader;

/// The standard RNG. This is designed to be efficient on the current
/// platform.
pub struct StdRng {
    rng: IsaacWordRng,
}

impl Copy for StdRng {}

impl StdRng {
    /// Create a randomly seeded instance of `StdRng`.
    ///
    /// This is a very expensive operation as it has to read
    /// randomness from the operating system and use this in an
    /// expensive seeding operation. If one is only generating a small
    /// number of random numbers, or doesn't need the utmost speed for
    /// generating each number, `task_rng` and/or `random` may be more
    /// appropriate.
    ///
    /// Reading the randomness from the OS may fail, and any error is
    /// propagated via the `IoResult` return value.
    pub fn new() -> IoResult<StdRng> {
        OsRng::new().map(|mut r| StdRng { rng: r.gen() })
    }
}

impl Rng for StdRng {
    #[inline]
    fn next_u32(&mut self) -> u32 {
        self.rng.next_u32()
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        self.rng.next_u64()
    }
}

impl<'a> SeedableRng<&'a [uint]> for StdRng {
    fn reseed(&mut self, seed: &'a [uint]) {
        // the internal RNG can just be seeded from the above
        // randomness.
        self.rng.reseed(unsafe {mem::transmute(seed)})
    }

    fn from_seed(seed: &'a [uint]) -> StdRng {
        StdRng { rng: SeedableRng::from_seed(unsafe {mem::transmute(seed)}) }
    }
}

/// Create a weak random number generator with a default algorithm and seed.
///
/// It returns the fastest `Rng` algorithm currently available in Rust without
/// consideration for cryptography or security. If you require a specifically
/// seeded `Rng` for consistency over time you should pick one algorithm and
/// create the `Rng` yourself.
///
/// This will read randomness from the operating system to seed the
/// generator.
pub fn weak_rng() -> XorShiftRng {
    match OsRng::new() {
        Ok(mut r) => r.gen(),
        Err(e) => panic!("weak_rng: failed to create seeded RNG: {}", e)
    }
}

/// Controls how the task-local RNG is reseeded.
struct TaskRngReseeder;

impl reseeding::Reseeder<StdRng> for TaskRngReseeder {
    fn reseed(&mut self, rng: &mut StdRng) {
        *rng = match StdRng::new() {
            Ok(r) => r,
            Err(e) => panic!("could not reseed task_rng: {}", e)
        }
    }
}
static TASK_RNG_RESEED_THRESHOLD: uint = 32_768;
type TaskRngInner = reseeding::ReseedingRng<StdRng, TaskRngReseeder>;

/// The task-local RNG.
pub struct TaskRng {
    rng: Rc<RefCell<TaskRngInner>>,
}

/// Retrieve the lazily-initialized task-local random number
/// generator, seeded by the system. Intended to be used in method
/// chaining style, e.g. `task_rng().gen::<int>()`.
///
/// The RNG provided will reseed itself from the operating system
/// after generating a certain amount of randomness.
///
/// The internal RNG used is platform and architecture dependent, even
/// if the operating system random number generator is rigged to give
/// the same sequence always. If absolute consistency is required,
/// explicitly select an RNG, e.g. `IsaacRng` or `Isaac64Rng`.
pub fn task_rng() -> TaskRng {
    // used to make space in TLS for a random number generator
    thread_local!(static TASK_RNG_KEY: Rc<RefCell<TaskRngInner>> = {
        let r = match StdRng::new() {
            Ok(r) => r,
            Err(e) => panic!("could not initialize task_rng: {}", e)
        };
        let rng = reseeding::ReseedingRng::new(r,
                                               TASK_RNG_RESEED_THRESHOLD,
                                               TaskRngReseeder);
        Rc::new(RefCell::new(rng))
    })

    TaskRng { rng: TASK_RNG_KEY.with(|t| t.clone()) }
}

impl Rng for TaskRng {
    fn next_u32(&mut self) -> u32 {
        self.rng.borrow_mut().next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.rng.borrow_mut().next_u64()
    }

    #[inline]
    fn fill_bytes(&mut self, bytes: &mut [u8]) {
        self.rng.borrow_mut().fill_bytes(bytes)
    }
}

/// Generates a random value using the task-local random number generator.
///
/// `random()` can generate various types of random things, and so may require
/// type hinting to generate the specific type you want.
///
/// # Examples
///
/// ```rust
/// use std::rand;
///
/// let x = rand::random();
/// println!("{}", 2u * x);
///
/// let y = rand::random::<f64>();
/// println!("{}", y);
///
/// if rand::random() { // generates a boolean
///     println!("Better lucky than good!");
/// }
/// ```
#[inline]
pub fn random<T: Rand>() -> T {
    task_rng().gen()
}

/// Randomly sample up to `amount` elements from an iterator.
///
/// # Example
///
/// ```rust
/// use std::rand::{task_rng, sample};
///
/// let mut rng = task_rng();
/// let sample = sample(&mut rng, range(1i, 100), 5);
/// println!("{}", sample);
/// ```
pub fn sample<T, I: Iterator<T>, R: Rng>(rng: &mut R,
                                         mut iter: I,
                                         amount: uint) -> Vec<T> {
    let mut reservoir: Vec<T> = iter.by_ref().take(amount).collect();
    for (i, elem) in iter.enumerate() {
        let k = rng.gen_range(0, i + 1 + amount);
        if k < amount {
            reservoir[k] = elem;
        }
    }
    return reservoir;
}

#[cfg(test)]
mod test {
    use prelude::*;
    use super::{Rng, task_rng, random, SeedableRng, StdRng, sample};
    use iter::order;

    struct ConstRng { i: u64 }
    impl Rng for ConstRng {
        fn next_u32(&mut self) -> u32 { self.i as u32 }
        fn next_u64(&mut self) -> u64 { self.i }

        // no fill_bytes on purpose
    }

    #[test]
    fn test_fill_bytes_default() {
        let mut r = ConstRng { i: 0x11_22_33_44_55_66_77_88 };

        // check every remainder mod 8, both in small and big vectors.
        let lengths = [0, 1, 2, 3, 4, 5, 6, 7,
                       80, 81, 82, 83, 84, 85, 86, 87];
        for &n in lengths.iter() {
            let mut v = Vec::from_elem(n, 0u8);
            r.fill_bytes(v.as_mut_slice());

            // use this to get nicer error messages.
            for (i, &byte) in v.iter().enumerate() {
                if byte == 0 {
                    panic!("byte {} of {} is zero", i, n)
                }
            }
        }
    }

    #[test]
    fn test_gen_range() {
        let mut r = task_rng();
        for _ in range(0u, 1000) {
            let a = r.gen_range(-3i, 42);
            assert!(a >= -3 && a < 42);
            assert_eq!(r.gen_range(0i, 1), 0);
            assert_eq!(r.gen_range(-12i, -11), -12);
        }

        for _ in range(0u, 1000) {
            let a = r.gen_range(10i, 42);
            assert!(a >= 10 && a < 42);
            assert_eq!(r.gen_range(0i, 1), 0);
            assert_eq!(r.gen_range(3_000_000u, 3_000_001), 3_000_000);
        }

    }

    #[test]
    #[should_fail]
    fn test_gen_range_panic_int() {
        let mut r = task_rng();
        r.gen_range(5i, -2);
    }

    #[test]
    #[should_fail]
    fn test_gen_range_panic_uint() {
        let mut r = task_rng();
        r.gen_range(5u, 2u);
    }

    #[test]
    fn test_gen_f64() {
        let mut r = task_rng();
        let a = r.gen::<f64>();
        let b = r.gen::<f64>();
        debug!("{}", (a, b));
    }

    #[test]
    fn test_gen_weighted_bool() {
        let mut r = task_rng();
        assert_eq!(r.gen_weighted_bool(0u), true);
        assert_eq!(r.gen_weighted_bool(1u), true);
    }

    #[test]
    fn test_gen_ascii_str() {
        let mut r = task_rng();
        assert_eq!(r.gen_ascii_chars().take(0).count(), 0u);
        assert_eq!(r.gen_ascii_chars().take(10).count(), 10u);
        assert_eq!(r.gen_ascii_chars().take(16).count(), 16u);
    }

    #[test]
    fn test_gen_vec() {
        let mut r = task_rng();
        assert_eq!(r.gen_iter::<u8>().take(0).count(), 0u);
        assert_eq!(r.gen_iter::<u8>().take(10).count(), 10u);
        assert_eq!(r.gen_iter::<f64>().take(16).count(), 16u);
    }

    #[test]
    fn test_choose() {
        let mut r = task_rng();
        assert_eq!(r.choose(&[1i, 1, 1]).map(|&x|x), Some(1));

        let v: &[int] = &[];
        assert_eq!(r.choose(v), None);
    }

    #[test]
    fn test_shuffle() {
        let mut r = task_rng();
        let empty: &mut [int] = &mut [];
        r.shuffle(empty);
        let mut one = [1i];
        r.shuffle(&mut one);
        let b: &[_] = &[1];
        assert_eq!(one, b);

        let mut two = [1i, 2];
        r.shuffle(&mut two);
        assert!(two == [1, 2] || two == [2, 1]);

        let mut x = [1i, 1, 1];
        r.shuffle(&mut x);
        let b: &[_] = &[1, 1, 1];
        assert_eq!(x, b);
    }

    #[test]
    fn test_task_rng() {
        let mut r = task_rng();
        r.gen::<int>();
        let mut v = [1i, 1, 1];
        r.shuffle(&mut v);
        let b: &[_] = &[1, 1, 1];
        assert_eq!(v, b);
        assert_eq!(r.gen_range(0u, 1u), 0u);
    }

    #[test]
    fn test_random() {
        // not sure how to test this aside from just getting some values
        let _n : uint = random();
        let _f : f32 = random();
        let _o : Option<Option<i8>> = random();
        let _many : ((),
                     (uint,
                      int,
                      Option<(u32, (bool,))>),
                     (u8, i8, u16, i16, u32, i32, u64, i64),
                     (f32, (f64, (f64,)))) = random();
    }

    #[test]
    fn test_sample() {
        let min_val = 1i;
        let max_val = 100i;

        let mut r = task_rng();
        let vals = range(min_val, max_val).collect::<Vec<int>>();
        let small_sample = sample(&mut r, vals.iter(), 5);
        let large_sample = sample(&mut r, vals.iter(), vals.len() + 5);

        assert_eq!(small_sample.len(), 5);
        assert_eq!(large_sample.len(), vals.len());

        assert!(small_sample.iter().all(|e| {
            **e >= min_val && **e <= max_val
        }));
    }

    #[test]
    fn test_std_rng_seeded() {
        let s = task_rng().gen_iter::<uint>().take(256).collect::<Vec<uint>>();
        let mut ra: StdRng = SeedableRng::from_seed(s.as_slice());
        let mut rb: StdRng = SeedableRng::from_seed(s.as_slice());
        assert!(order::equals(ra.gen_ascii_chars().take(100),
                              rb.gen_ascii_chars().take(100)));
    }

    #[test]
    fn test_std_rng_reseed() {
        let s = task_rng().gen_iter::<uint>().take(256).collect::<Vec<uint>>();
        let mut r: StdRng = SeedableRng::from_seed(s.as_slice());
        let string1 = r.gen_ascii_chars().take(100).collect::<String>();

        r.reseed(s.as_slice());

        let string2 = r.gen_ascii_chars().take(100).collect::<String>();
        assert_eq!(string1, string2);
    }
}

#[cfg(test)]
static RAND_BENCH_N: u64 = 100;

#[cfg(test)]
mod bench {
    extern crate test;
    use prelude::*;

    use self::test::Bencher;
    use super::{XorShiftRng, StdRng, IsaacRng, Isaac64Rng, Rng, RAND_BENCH_N};
    use super::{OsRng, weak_rng};
    use mem::size_of;

    #[bench]
    fn rand_xorshift(b: &mut Bencher) {
        let mut rng: XorShiftRng = OsRng::new().unwrap().gen();
        b.iter(|| {
            for _ in range(0, RAND_BENCH_N) {
                rng.gen::<uint>();
            }
        });
        b.bytes = size_of::<uint>() as u64 * RAND_BENCH_N;
    }

    #[bench]
    fn rand_isaac(b: &mut Bencher) {
        let mut rng: IsaacRng = OsRng::new().unwrap().gen();
        b.iter(|| {
            for _ in range(0, RAND_BENCH_N) {
                rng.gen::<uint>();
            }
        });
        b.bytes = size_of::<uint>() as u64 * RAND_BENCH_N;
    }

    #[bench]
    fn rand_isaac64(b: &mut Bencher) {
        let mut rng: Isaac64Rng = OsRng::new().unwrap().gen();
        b.iter(|| {
            for _ in range(0, RAND_BENCH_N) {
                rng.gen::<uint>();
            }
        });
        b.bytes = size_of::<uint>() as u64 * RAND_BENCH_N;
    }

    #[bench]
    fn rand_std(b: &mut Bencher) {
        let mut rng = StdRng::new().unwrap();
        b.iter(|| {
            for _ in range(0, RAND_BENCH_N) {
                rng.gen::<uint>();
            }
        });
        b.bytes = size_of::<uint>() as u64 * RAND_BENCH_N;
    }

    #[bench]
    fn rand_shuffle_100(b: &mut Bencher) {
        let mut rng = weak_rng();
        let x : &mut[uint] = &mut [1,..100];
        b.iter(|| {
            rng.shuffle(x);
        })
    }
}
