// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!
Random number generation.

The key functions are `random()` and `RngUtil::gen()`. These are polymorphic
and so can be used to generate any type that implements `Rand`. Type inference
means that often a simple call to `rand::random()` or `rng.gen()` will
suffice, but sometimes an annotation is required, e.g. `rand::random::<float>()`.

# Examples
~~~
use core::rand::RngUtil;

fn main() {
    let rng = rand::rng();
    if rng.gen() { // bool
        println(fmt!("int: %d, uint: %u", rng.gen(), rng.gen()))
    }
}
~~~

~~~
fn main () {
    let tuple_ptr = rand::random::<~(f64, char)>();
    println(fmt!("%?", tuple_ptr))
}
~~~
*/


use int;
use prelude::*;
use str;
use task;
use u32;
use uint;
use util;
use vec;
use libc::size_t;

/// A type that can be randomly generated using an Rng
pub trait Rand {
    fn rand<R: Rng>(rng: &R) -> Self;
}

impl Rand for int {
    fn rand<R: Rng>(rng: &R) -> int {
        if int::bits == 32 {
            rng.next() as int
        } else {
            rng.gen::<i64>() as int
        }
    }
}

impl Rand for i8 {
    fn rand<R: Rng>(rng: &R) -> i8 {
        rng.next() as i8
    }
}

impl Rand for i16 {
    fn rand<R: Rng>(rng: &R) -> i16 {
        rng.next() as i16
    }
}

impl Rand for i32 {
    fn rand<R: Rng>(rng: &R) -> i32 {
        rng.next() as i32
    }
}

impl Rand for i64 {
    fn rand<R: Rng>(rng: &R) -> i64 {
        (rng.next() as i64 << 32) | rng.next() as i64
    }
}

impl Rand for uint {
    fn rand<R: Rng>(rng: &R) -> uint {
        if uint::bits == 32 {
            rng.next() as uint
        } else {
            rng.gen::<u64>() as uint
        }
    }
}

impl Rand for u8 {
    fn rand<R: Rng>(rng: &R) -> u8 {
        rng.next() as u8
    }
}

impl Rand for u16 {
    fn rand<R: Rng>(rng: &R) -> u16 {
        rng.next() as u16
    }
}

impl Rand for u32 {
    fn rand<R: Rng>(rng: &R) -> u32 {
        rng.next()
    }
}

impl Rand for u64 {
    fn rand<R: Rng>(rng: &R) -> u64 {
        (rng.next() as u64 << 32) | rng.next() as u64
    }
}

impl Rand for float {
    fn rand<R: Rng>(rng: &R) -> float {
        rng.gen::<f64>() as float
    }
}

impl Rand for f32 {
    fn rand<R: Rng>(rng: &R) -> f32 {
        rng.gen::<f64>() as f32
    }
}

static scale : f64 = (u32::max_value as f64) + 1.0f64;
impl Rand for f64 {
    fn rand<R: Rng>(rng: &R) -> f64 {
        let u1 = rng.next() as f64;
        let u2 = rng.next() as f64;
        let u3 = rng.next() as f64;

        ((u1 / scale + u2) / scale + u3) / scale
    }
}

impl Rand for char {
    fn rand<R: Rng>(rng: &R) -> char {
        rng.next() as char
    }
}

impl Rand for bool {
    fn rand<R: Rng>(rng: &R) -> bool {
        rng.next() & 1u32 == 1u32
    }
}

macro_rules! tuple_impl {
    // use variables to indicate the arity of the tuple
    ($($tyvar:ident),* ) => {
        // the trailing commas are for the 1 tuple
        impl<
            $( $tyvar : Rand ),*
            > Rand for ( $( $tyvar ),* , ) {

            fn rand<R: Rng>(_rng: &R) -> ( $( $tyvar ),* , ) {
                (
                    // use the $tyvar's to get the appropriate number of
                    // repeats (they're not actually needed)
                    $(
                        _rng.gen::<$tyvar>()
                    ),*
                    ,
                )
            }
        }
    }
}

impl Rand for () { fn rand<R: Rng>(_: &R) -> () { () } }
tuple_impl!{A}
tuple_impl!{A, B}
tuple_impl!{A, B, C}
tuple_impl!{A, B, C, D}
tuple_impl!{A, B, C, D, E}
tuple_impl!{A, B, C, D, E, F}
tuple_impl!{A, B, C, D, E, F, G}
tuple_impl!{A, B, C, D, E, F, G, H}
tuple_impl!{A, B, C, D, E, F, G, H, I}
tuple_impl!{A, B, C, D, E, F, G, H, I, J}

impl<T:Rand> Rand for Option<T> {
    fn rand<R: Rng>(rng: &R) -> Option<T> {
        if rng.gen() {
            Some(rng.gen())
        } else {
            None
        }
    }
}

impl<T: Rand> Rand for ~T {
    fn rand<R: Rng>(rng: &R) -> ~T { ~rng.gen() }
}

impl<T: Rand> Rand for @T {
    fn rand<R: Rng>(rng: &R) -> @T { @rng.gen() }
}

#[allow(non_camel_case_types)] // runtime type
pub enum rust_rng {}

#[abi = "cdecl"]
pub mod rustrt {
    use libc::size_t;
    use super::rust_rng;

    pub extern {
        unsafe fn rand_seed_size() -> size_t;
        unsafe fn rand_gen_seed(buf: *mut u8, sz: size_t);
        unsafe fn rand_new_seeded(buf: *u8, sz: size_t) -> *rust_rng;
        unsafe fn rand_next(rng: *rust_rng) -> u32;
        unsafe fn rand_free(rng: *rust_rng);
    }
}

/// A random number generator
pub trait Rng {
    /// Return the next random integer
    pub fn next(&self) -> u32;
}

/// A value with a particular weight compared to other values
pub struct Weighted<T> {
    weight: uint,
    item: T,
}

pub trait RngUtil {
    /// Return a random value of a Rand type
    fn gen<T:Rand>(&self) -> T;
    /**
     * Return a int randomly chosen from the range [start, end),
     * failing if start >= end
     */
    fn gen_int_range(&self, start: int, end: int) -> int;
    /**
     * Return a uint randomly chosen from the range [start, end),
     * failing if start >= end
     */
    fn gen_uint_range(&self, start: uint, end: uint) -> uint;
    /**
     * Return a char randomly chosen from chars, failing if chars is empty
     */
    fn gen_char_from(&self, chars: &str) -> char;
    /**
     * Return a bool with a 1 in n chance of true
     *
     * *Example*
     *
     * ~~~
     *
     * use core::rand::RngUtil;
     *
     * fn main() {
     *     rng = rand::rng();
     *     println(fmt!("%b",rng.gen_weighted_bool(3)));
     * }
     * ~~~
     */
    fn gen_weighted_bool(&self, n: uint) -> bool;
    /**
     * Return a random string of the specified length composed of A-Z,a-z,0-9
     *
     * *Example*
     *
     * ~~~
     *
     * use core::rand::RngUtil;
     *
     * fn main() {
     *     rng = rand::rng();
     *     println(rng.gen_str(8));
     * }
     * ~~~
     */
    fn gen_str(&self, len: uint) -> ~str;
    /**
     * Return a random byte string of the specified length
     *
     * *Example*
     *
     * ~~~
     *
     * use core::rand::RngUtil;
     *
     * fn main() {
     *     rng = rand::rng();
     *     println(fmt!("%?",rng.gen_bytes(8)));
     * }
     * ~~~
     */
    fn gen_bytes(&self, len: uint) -> ~[u8];
    /**
     * Choose an item randomly, failing if values is empty
     *
     * *Example*
     *
     * ~~~
     *
     * use core::rand::RngUtil;
     *
     * fn main() {
     *     rng = rand::rng();
     *     println(fmt!("%d",rng.choose([1,2,4,8,16,32])));
     * }
     * ~~~
     */
    fn choose<T:Copy>(&self, values: &[T]) -> T;
    /// Choose Some(item) randomly, returning None if values is empty
    fn choose_option<T:Copy>(&self, values: &[T]) -> Option<T>;
    /**
     * Choose an item respecting the relative weights, failing if the sum of
     * the weights is 0
     *
     * *Example*
     *
     * ~~~
     *
     * use core::rand::RngUtil;
     *
     * fn main() {
     *     rng = rand::rng();
     *     let x = [rand::Weighted {weight: 4, item: 'a'},
     *              rand::Weighted {weight: 2, item: 'b'},
     *              rand::Weighted {weight: 2, item: 'c'}];
     *     println(fmt!("%c",rng.choose_weighted(x)));
     * }
     * ~~~
     */
    fn choose_weighted<T:Copy>(&self, v : &[Weighted<T>]) -> T;
    /**
     * Choose Some(item) respecting the relative weights, returning none if
     * the sum of the weights is 0
     *
     * *Example*
     *
     * ~~~
     *
     * use core::rand::RngUtil;
     *
     * fn main() {
     *     rng = rand::rng();
     *     let x = [rand::Weighted {weight: 4, item: 'a'},
     *              rand::Weighted {weight: 2, item: 'b'},
     *              rand::Weighted {weight: 2, item: 'c'}];
     *     println(fmt!("%?",rng.choose_weighted_option(x)));
     * }
     * ~~~
     */
    fn choose_weighted_option<T:Copy>(&self, v: &[Weighted<T>]) -> Option<T>;
    /**
     * Return a vec containing copies of the items, in order, where
     * the weight of the item determines how many copies there are
     *
     * *Example*
     *
     * ~~~
     *
     * use core::rand::RngUtil;
     *
     * fn main() {
     *     rng = rand::rng();
     *     let x = [rand::Weighted {weight: 4, item: 'a'},
     *              rand::Weighted {weight: 2, item: 'b'},
     *              rand::Weighted {weight: 2, item: 'c'}];
     *     println(fmt!("%?",rng.weighted_vec(x)));
     * }
     * ~~~
     */
    fn weighted_vec<T:Copy>(&self, v: &[Weighted<T>]) -> ~[T];
    /**
     * Shuffle a vec
     *
     * *Example*
     *
     * ~~~
     *
     * use core::rand::RngUtil;
     *
     * fn main() {
     *     rng = rand::rng();
     *     println(fmt!("%?",rng.shuffle([1,2,3])));
     * }
     * ~~~
     */
    fn shuffle<T:Copy>(&self, values: &[T]) -> ~[T];
    /**
     * Shuffle a mutable vec in place
     *
     * *Example*
     *
     * ~~~
     *
     * use core::rand::RngUtil;
     *
     * fn main() {
     *     rng = rand::rng();
     *     let mut y = [1,2,3];
     *     rng.shuffle_mut(y);
     *     println(fmt!("%?",y));
     *     rng.shuffle_mut(y);
     *     println(fmt!("%?",y));
     * }
     * ~~~
     */
    fn shuffle_mut<T>(&self, values: &mut [T]);
}

/// Extension methods for random number generators
impl<R: Rng> RngUtil for R {
    /// Return a random value for a Rand type
    fn gen<T: Rand>(&self) -> T {
        Rand::rand(self)
    }

    /**
     * Return an int randomly chosen from the range [start, end),
     * failing if start >= end
     */
    fn gen_int_range(&self, start: int, end: int) -> int {
        assert!(start < end);
        start + int::abs(self.gen::<int>() % (end - start))
    }

    /**
     * Return a uint randomly chosen from the range [start, end),
     * failing if start >= end
     */
    fn gen_uint_range(&self, start: uint, end: uint) -> uint {
        assert!(start < end);
        start + (self.gen::<uint>() % (end - start))
    }

    /**
     * Return a char randomly chosen from chars, failing if chars is empty
     */
    fn gen_char_from(&self, chars: &str) -> char {
        assert!(!chars.is_empty());
        let mut cs = ~[];
        for str::each_char(chars) |c| { cs.push(c) }
        self.choose(cs)
    }

    /// Return a bool with a 1-in-n chance of true
    fn gen_weighted_bool(&self, n: uint) -> bool {
        if n == 0u {
            true
        } else {
            self.gen_uint_range(1u, n + 1u) == 1u
        }
    }

    /**
     * Return a random string of the specified length composed of A-Z,a-z,0-9
     */
    fn gen_str(&self, len: uint) -> ~str {
        let charset = ~"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                       abcdefghijklmnopqrstuvwxyz\
                       0123456789";
        let mut s = ~"";
        let mut i = 0u;
        while (i < len) {
            s = s + str::from_char(self.gen_char_from(charset));
            i += 1u;
        }
        s
    }

    /// Return a random byte string of the specified length
    fn gen_bytes(&self, len: uint) -> ~[u8] {
        do vec::from_fn(len) |_i| {
            self.gen()
        }
    }

    /// Choose an item randomly, failing if values is empty
    fn choose<T:Copy>(&self, values: &[T]) -> T {
        self.choose_option(values).get()
    }

    /// Choose Some(item) randomly, returning None if values is empty
    fn choose_option<T:Copy>(&self, values: &[T]) -> Option<T> {
        if values.is_empty() {
            None
        } else {
            Some(values[self.gen_uint_range(0u, values.len())])
        }
    }
    /**
     * Choose an item respecting the relative weights, failing if the sum of
     * the weights is 0
     */
    fn choose_weighted<T:Copy>(&self, v : &[Weighted<T>]) -> T {
        self.choose_weighted_option(v).get()
    }

    /**
     * Choose Some(item) respecting the relative weights, returning none if
     * the sum of the weights is 0
     */
    fn choose_weighted_option<T:Copy>(&self, v: &[Weighted<T>]) -> Option<T> {
        let mut total = 0u;
        for v.each |item| {
            total += item.weight;
        }
        if total == 0u {
            return None;
        }
        let chosen = self.gen_uint_range(0u, total);
        let mut so_far = 0u;
        for v.each |item| {
            so_far += item.weight;
            if so_far > chosen {
                return Some(item.item);
            }
        }
        util::unreachable();
    }

    /**
     * Return a vec containing copies of the items, in order, where
     * the weight of the item determines how many copies there are
     */
    fn weighted_vec<T:Copy>(&self, v: &[Weighted<T>]) -> ~[T] {
        let mut r = ~[];
        for v.each |item| {
            for uint::range(0u, item.weight) |_i| {
                r.push(item.item);
            }
        }
        r
    }

    /// Shuffle a vec
    fn shuffle<T:Copy>(&self, values: &[T]) -> ~[T] {
        let mut m = vec::from_slice(values);
        self.shuffle_mut(m);
        m
    }

    /// Shuffle a mutable vec in place
    fn shuffle_mut<T>(&self, values: &mut [T]) {
        let mut i = values.len();
        while i >= 2u {
            // invariant: elements with index >= i have been locked in place.
            i -= 1u;
            // lock element i in place.
            vec::swap(values, i, self.gen_uint_range(0u, i + 1u));
        }
    }
}

/// Create a random number generator with a default algorithm and seed.
pub fn rng() -> IsaacRng {
    IsaacRng::new()
}

pub struct IsaacRng {
    priv rng: *rust_rng,
}

impl Drop for IsaacRng {
    fn finalize(&self) {
        unsafe {
            rustrt::rand_free(self.rng);
        }
    }
}

pub impl IsaacRng {
    priv fn from_rust_rng(rng: *rust_rng) -> IsaacRng {
        IsaacRng {
            rng: rng
        }
    }

    /// Create an ISAAC random number generator with a system specified seed
    fn new() -> IsaacRng {
        IsaacRng::new_seeded(seed())
    }

    /**
     * Create a random number generator using the specified seed. A generator
     * constructed with a given seed will generate the same sequence of values as
     * all other generators constructed with the same seed. The seed may be any
     * length.
     */
    fn new_seeded(seed: &[u8]) -> IsaacRng {
        unsafe {
            do vec::as_imm_buf(seed) |p, sz| {
                IsaacRng::from_rust_rng(rustrt::rand_new_seeded(p, sz as size_t))
            }
        }
    }
}

impl Rng for IsaacRng {
    pub fn next(&self) -> u32 {
        unsafe {
            return rustrt::rand_next(self.rng);
        }
    }
}

/// Create a new random seed for IsaacRng::new_seeded
pub fn seed() -> ~[u8] {
    unsafe {
        let n = rustrt::rand_seed_size() as uint;
        let mut s = vec::from_elem(n, 0_u8);
        do vec::as_mut_buf(s) |p, sz| {
            rustrt::rand_gen_seed(p, sz as size_t)
        }
        s
    }
}

struct XorShiftRng {
    priv mut x: u32,
    priv mut y: u32,
    priv mut z: u32,
    priv mut w: u32,
}

impl Rng for XorShiftRng {
    pub fn next(&self) -> u32 {
        let x = self.x;
        let t = x ^ (x << 11);
        self.x = self.y;
        self.y = self.z;
        self.z = self.w;
        let w = self.w;
        self.w = w ^ (w >> 19) ^ (t ^ (t >> 8));
        self.w
    }
}

pub impl XorShiftRng {
    /// Create an xor shift random number generator with a default seed.
    fn new() -> XorShiftRng {
        // constants taken from http://en.wikipedia.org/wiki/Xorshift
        XorShiftRng::new_seeded(123456789u32, 362436069u32, 521288629u32, 88675123u32)
    }

    /**
     * Create a random number generator using the specified seed. A generator
     * constructed with a given seed will generate the same sequence of values as
     * all other generators constructed with the same seed.
     */
    fn new_seeded(x: u32, y: u32, z: u32, w: u32) -> XorShiftRng {
        XorShiftRng { x: x, y: y, z: z, w: w }
    }

}

// used to make space in TLS for a random number generator
fn tls_rng_state(_v: @IsaacRng) {}

/**
 * Gives back a lazily initialized task-local random number generator,
 * seeded by the system. Intended to be used in method chaining style, ie
 * `task_rng().gen::<int>()`.
 */
pub fn task_rng() -> @IsaacRng {
    let r : Option<@IsaacRng>;
    unsafe {
        r = task::local_data::local_data_get(tls_rng_state);
    }
    match r {
        None => {
            unsafe {
                let rng = @IsaacRng::new_seeded(seed());
                task::local_data::local_data_set(tls_rng_state, rng);
                rng
            }
        }
        Some(rng) => rng
    }
}

// Allow direct chaining with `task_rng`
impl<R: Rng> Rng for @R {
    fn next(&self) -> u32 { (**self).next() }
}

/**
 * Returns a random value of a Rand type, using the task's random number
 * generator.
 */
pub fn random<T: Rand>() -> T {
    (*task_rng()).gen()
}

#[cfg(test)]
mod tests {
    use option::{Option, Some};
    use super::*;

    #[test]
    fn test_rng_seeded() {
        let seed = seed();
        let ra = IsaacRng::new_seeded(seed);
        let rb = IsaacRng::new_seeded(seed);
        assert!(ra.gen_str(100u) == rb.gen_str(100u));
    }

    #[test]
    fn test_rng_seeded_custom_seed() {
        // much shorter than generated seeds which are 1024 bytes
        let seed = [2u8, 32u8, 4u8, 32u8, 51u8];
        let ra = IsaacRng::new_seeded(seed);
        let rb = IsaacRng::new_seeded(seed);
        assert!(ra.gen_str(100u) == rb.gen_str(100u));
    }

    #[test]
    fn test_rng_seeded_custom_seed2() {
        let seed = [2u8, 32u8, 4u8, 32u8, 51u8];
        let ra = IsaacRng::new_seeded(seed);
        // Regression test that isaac is actually using the above vector
        let r = ra.next();
        error!("%?", r);
        assert!(r == 890007737u32 // on x86_64
                     || r == 2935188040u32); // on x86
    }

    #[test]
    fn test_gen_int_range() {
        let r = rng();
        let a = r.gen_int_range(-3, 42);
        assert!(a >= -3 && a < 42);
        assert!(r.gen_int_range(0, 1) == 0);
        assert!(r.gen_int_range(-12, -11) == -12);
    }

    #[test]
    #[should_fail]
    #[ignore(cfg(windows))]
    fn test_gen_int_from_fail() {
        rng().gen_int_range(5, -2);
    }

    #[test]
    fn test_gen_uint_range() {
        let r = rng();
        let a = r.gen_uint_range(3u, 42u);
        assert!(a >= 3u && a < 42u);
        assert!(r.gen_uint_range(0u, 1u) == 0u);
        assert!(r.gen_uint_range(12u, 13u) == 12u);
    }

    #[test]
    #[should_fail]
    #[ignore(cfg(windows))]
    fn test_gen_uint_range_fail() {
        rng().gen_uint_range(5u, 2u);
    }

    #[test]
    fn test_gen_float() {
        let r = rng();
        let a = r.gen::<float>();
        let b = r.gen::<float>();
        debug!((a, b));
    }

    #[test]
    fn test_gen_weighted_bool() {
        let r = rng();
        assert!(r.gen_weighted_bool(0u) == true);
        assert!(r.gen_weighted_bool(1u) == true);
    }

    #[test]
    fn test_gen_str() {
        let r = rng();
        debug!(r.gen_str(10u));
        debug!(r.gen_str(10u));
        debug!(r.gen_str(10u));
        assert!(r.gen_str(0u).len() == 0u);
        assert!(r.gen_str(10u).len() == 10u);
        assert!(r.gen_str(16u).len() == 16u);
    }

    #[test]
    fn test_gen_bytes() {
        let r = rng();
        assert!(r.gen_bytes(0u).len() == 0u);
        assert!(r.gen_bytes(10u).len() == 10u);
        assert!(r.gen_bytes(16u).len() == 16u);
    }

    #[test]
    fn test_choose() {
        let r = rng();
        assert!(r.choose([1, 1, 1]) == 1);
    }

    #[test]
    fn test_choose_option() {
        let r = rng();
        let x: Option<int> = r.choose_option([]);
        assert!(x.is_none());
        assert!(r.choose_option([1, 1, 1]) == Some(1));
    }

    #[test]
    fn test_choose_weighted() {
        let r = rng();
        assert!(r.choose_weighted(~[
            Weighted { weight: 1u, item: 42 },
        ]) == 42);
        assert!(r.choose_weighted(~[
            Weighted { weight: 0u, item: 42 },
            Weighted { weight: 1u, item: 43 },
        ]) == 43);
    }

    #[test]
    fn test_choose_weighted_option() {
        let r = rng();
        assert!(r.choose_weighted_option(~[
            Weighted { weight: 1u, item: 42 },
        ]) == Some(42));
        assert!(r.choose_weighted_option(~[
            Weighted { weight: 0u, item: 42 },
            Weighted { weight: 1u, item: 43 },
        ]) == Some(43));
        let v: Option<int> = r.choose_weighted_option([]);
        assert!(v.is_none());
    }

    #[test]
    fn test_weighted_vec() {
        let r = rng();
        let empty: ~[int] = ~[];
        assert!(r.weighted_vec(~[]) == empty);
        assert!(r.weighted_vec(~[
            Weighted { weight: 0u, item: 3u },
            Weighted { weight: 1u, item: 2u },
            Weighted { weight: 2u, item: 1u },
        ]) == ~[2u, 1u, 1u]);
    }

    #[test]
    fn test_shuffle() {
        let r = rng();
        let empty: ~[int] = ~[];
        assert!(r.shuffle(~[]) == empty);
        assert!(r.shuffle(~[1, 1, 1]) == ~[1, 1, 1]);
    }

    #[test]
    fn test_task_rng() {
        let r = task_rng();
        r.gen::<int>();
        assert!(r.shuffle(~[1, 1, 1]) == ~[1, 1, 1]);
        assert!(r.gen_uint_range(0u, 1u) == 0u);
    }

    #[test]
    fn test_random() {
        // not sure how to test this aside from just getting some values
        let _n : uint = random();
        let _f : f32 = random();
        let _o : Option<Option<i8>> = random();
        let _many : ((),
                     (~uint, @int, ~Option<~(@char, ~(@bool,))>),
                     (u8, i8, u16, i16, u32, i32, u64, i64),
                     (f32, (f64, (float,)))) = random();
    }
}


// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
