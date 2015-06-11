// Copyright 2014-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Thread local storage

#![unstable(feature = "thread_local_internals")]

use prelude::v1::*;

use cell::UnsafeCell;

// Sure wish we had macro hygiene, no?
#[doc(hidden)]
pub use self::imp::Key as __KeyInner;

/// A thread local storage key which owns its contents.
///
/// This key uses the fastest possible implementation available to it for the
/// target platform. It is instantiated with the `thread_local!` macro and the
/// primary method is the `with` method.
///
/// The `with` method yields a reference to the contained value which cannot be
/// sent across threads or escape the given closure.
///
/// # Initialization and Destruction
///
/// Initialization is dynamically performed on the first call to `with()`
/// within a thread, and values support destructors which will be run when a
/// thread exits.
///
/// # Examples
///
/// ```
/// use std::cell::RefCell;
/// use std::thread;
///
/// thread_local!(static FOO: RefCell<u32> = RefCell::new(1));
///
/// FOO.with(|f| {
///     assert_eq!(*f.borrow(), 1);
///     *f.borrow_mut() = 2;
/// });
///
/// // each thread starts out with the initial value of 1
/// thread::spawn(move|| {
///     FOO.with(|f| {
///         assert_eq!(*f.borrow(), 1);
///         *f.borrow_mut() = 3;
///     });
/// });
///
/// // we retain our original value of 2 despite the child thread
/// FOO.with(|f| {
///     assert_eq!(*f.borrow(), 2);
/// });
/// ```
#[stable(feature = "rust1", since = "1.0.0")]
pub struct LocalKey<T> {
    // The key itself may be tagged with #[thread_local], and this `Key` is
    // stored as a `static`, and it's not valid for a static to reference the
    // address of another thread_local static. For this reason we kinda wonkily
    // work around this by generating a shim function which will give us the
    // address of the inner TLS key at runtime.
    //
    // This is trivially devirtualizable by LLVM because we never store anything
    // to this field and rustc can declare the `static` as constant as well.
    inner: fn() -> &'static __KeyInner<T>,

    // initialization routine to invoke to create a value
    init: fn() -> T,
}

// Macro pain #4586:
//
// When cross compiling, rustc will load plugins and macros from the *host*
// platform before search for macros from the target platform. This is primarily
// done to detect, for example, plugins. Ideally the macro below would be
// defined once per module below, but unfortunately this means we have the
// following situation:
//
// 1. We compile libstd for x86_64-unknown-linux-gnu, this thread_local!() macro
//    will inject #[thread_local] statics.
// 2. We then try to compile a program for arm-linux-androideabi
// 3. The compiler has a host of linux and a target of android, so it loads
//    macros from the *linux* libstd.
// 4. The macro generates a #[thread_local] field, but the android libstd does
//    not use #[thread_local]
// 5. Compile error about structs with wrong fields.
//
// To get around this, we're forced to inject the #[cfg] logic into the macro
// itself. Woohoo.

/// Declare a new thread local storage key of type `std::thread::LocalKey`.
///
/// See [LocalKey documentation](thread/struct.LocalKey.html) for more
/// information.
#[macro_export]
#[stable(feature = "rust1", since = "1.0.0")]
#[allow_internal_unstable]
#[cfg(not(no_elf_tls))]
macro_rules! thread_local {
    (static $name:ident: $t:ty = $init:expr) => (
        static $name: ::std::thread::LocalKey<$t> =
            __thread_local_inner!($t, $init,
                #[cfg_attr(all(any(target_os = "macos", target_os = "linux"),
                               not(target_arch = "aarch64")),
                           thread_local)]);
    );
    (pub static $name:ident: $t:ty = $init:expr) => (
        pub static $name: ::std::thread::LocalKey<$t> =
            __thread_local_inner!($t, $init,
                #[cfg_attr(all(any(target_os = "macos", target_os = "linux"),
                               not(target_arch = "aarch64")),
                           thread_local)]);
    );
}

#[macro_export]
#[stable(feature = "rust1", since = "1.0.0")]
#[allow_internal_unstable]
#[cfg(no_elf_tls)]
macro_rules! thread_local {
    (static $name:ident: $t:ty = $init:expr) => (
        static $name: ::std::thread::LocalKey<$t> =
            __thread_local_inner!($t, $init, #[]);
    );
    (pub static $name:ident: $t:ty = $init:expr) => (
        pub static $name: ::std::thread::LocalKey<$t> =
            __thread_local_inner!($t, $init, #[]);
    );
}

#[doc(hidden)]
#[unstable(feature = "thread_local_internals",
           reason = "should not be necessary")]
#[macro_export]
#[allow_internal_unstable]
macro_rules! __thread_local_inner {
    ($t:ty, $init:expr, #[$($attr:meta),*]) => {{
        $(#[$attr])*
        static __KEY: ::std::thread::__LocalKeyInner<$t> =
            ::std::thread::__LocalKeyInner::new();
        fn __init() -> $t { $init }
        fn __getit() -> &'static ::std::thread::__LocalKeyInner<$t> { &__KEY }
        ::std::thread::LocalKey::new(__getit, __init)
    }}
}

/// Indicator of the state of a thread local storage key.
#[unstable(feature = "std_misc",
           reason = "state querying was recently added")]
#[derive(Eq, PartialEq, Copy, Clone)]
pub enum LocalKeyState {
    /// All keys are in this state whenever a thread starts. Keys will
    /// transition to the `Valid` state once the first call to `with` happens
    /// and the initialization expression succeeds.
    ///
    /// Keys in the `Uninitialized` state will yield a reference to the closure
    /// passed to `with` so long as the initialization routine does not panic.
    Uninitialized,

    /// Once a key has been accessed successfully, it will enter the `Valid`
    /// state. Keys in the `Valid` state will remain so until the thread exits,
    /// at which point the destructor will be run and the key will enter the
    /// `Destroyed` state.
    ///
    /// Keys in the `Valid` state will be guaranteed to yield a reference to the
    /// closure passed to `with`.
    Valid,

    /// When a thread exits, the destructors for keys will be run (if
    /// necessary). While a destructor is running, and possibly after a
    /// destructor has run, a key is in the `Destroyed` state.
    ///
    /// Keys in the `Destroyed` states will trigger a panic when accessed via
    /// `with`.
    Destroyed,
}

impl<T: 'static> LocalKey<T> {
    #[doc(hidden)]
    #[unstable(feature = "thread_local_internals",
               reason = "recently added to create a key")]
    pub const fn new(inner: fn() -> &'static __KeyInner<T>,
                     init: fn() -> T) -> LocalKey<T> {
        LocalKey {
            inner: inner,
            init: init
        }
    }

    /// Acquires a reference to the value in this TLS key.
    ///
    /// This will lazily initialize the value if this thread has not referenced
    /// this key yet.
    ///
    /// # Panics
    ///
    /// This function will `panic!()` if the key currently has its
    /// destructor running, and it **may** panic if the destructor has
    /// previously been run for this thread.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn with<F, R>(&'static self, f: F) -> R
                      where F: FnOnce(&T) -> R {
        let slot = (self.inner)();
        unsafe {
            let slot = slot.get().expect("cannot access a TLS value during or \
                                          after it is destroyed");
            f(match *slot.get() {
                Some(ref inner) => inner,
                None => self.init(slot),
            })
        }
    }

    unsafe fn init(&self, slot: &UnsafeCell<Option<T>>) -> &T {
        // Execute the initialization up front, *then* move it into our slot,
        // just in case initialization fails.
        let value = (self.init)();
        let ptr = slot.get();
        *ptr = Some(value);
        (*ptr).as_ref().unwrap()
    }

    /// Query the current state of this key.
    ///
    /// A key is initially in the `Uninitialized` state whenever a thread
    /// starts. It will remain in this state up until the first call to `with`
    /// within a thread has run the initialization expression successfully.
    ///
    /// Once the initialization expression succeeds, the key transitions to the
    /// `Valid` state which will guarantee that future calls to `with` will
    /// succeed within the thread.
    ///
    /// When a thread exits, each key will be destroyed in turn, and as keys are
    /// destroyed they will enter the `Destroyed` state just before the
    /// destructor starts to run. Keys may remain in the `Destroyed` state after
    /// destruction has completed. Keys without destructors (e.g. with types
    /// that are `Copy`), may never enter the `Destroyed` state.
    ///
    /// Keys in the `Uninitialized` can be accessed so long as the
    /// initialization does not panic. Keys in the `Valid` state are guaranteed
    /// to be able to be accessed. Keys in the `Destroyed` state will panic on
    /// any call to `with`.
    #[unstable(feature = "std_misc",
               reason = "state querying was recently added")]
    pub fn state(&'static self) -> LocalKeyState {
        unsafe {
            match (self.inner)().get() {
                Some(cell) => {
                    match *cell.get() {
                        Some(..) => LocalKeyState::Valid,
                        None => LocalKeyState::Uninitialized,
                    }
                }
                None => LocalKeyState::Destroyed,
            }
        }
    }
}

#[cfg(all(any(target_os = "macos", target_os = "linux"),
          not(target_arch = "aarch64"),
          not(no_elf_tls)))]
#[doc(hidden)]
mod imp {
    use prelude::v1::*;

    use cell::{Cell, UnsafeCell};
    use intrinsics;

    pub struct Key<T> {
        inner: UnsafeCell<Option<T>>,

        // Metadata to keep track of the state of the destructor. Remember that
        // these variables are thread-local, not global.
        dtor_registered: Cell<bool>,
        dtor_running: Cell<bool>,
    }

    unsafe impl<T> ::marker::Sync for Key<T> { }

    impl<T> Key<T> {
        pub const fn new() -> Key<T> {
            Key {
                inner: UnsafeCell::new(None),
                dtor_registered: Cell::new(false),
                dtor_running: Cell::new(false)
            }
        }

        pub unsafe fn get(&'static self) -> Option<&'static UnsafeCell<Option<T>>> {
            if intrinsics::needs_drop::<T>() && self.dtor_running.get() {
                return None
            }
            self.register_dtor();
            Some(&self.inner)
        }

        unsafe fn register_dtor(&self) {
            if !intrinsics::needs_drop::<T>() || self.dtor_registered.get() {
                return
            }

            register_dtor(self as *const _ as *mut u8,
                          destroy_value::<T>);
            self.dtor_registered.set(true);
        }
    }

    // Since what appears to be glibc 2.18 this symbol has been shipped which
    // GCC and clang both use to invoke destructors in thread_local globals, so
    // let's do the same!
    //
    // Note, however, that we run on lots older linuxes, as well as cross
    // compiling from a newer linux to an older linux, so we also have a
    // fallback implementation to use as well.
    //
    // Due to rust-lang/rust#18804, make sure this is not generic!
    #[cfg(target_os = "linux")]
    unsafe fn register_dtor(t: *mut u8, dtor: unsafe extern fn(*mut u8)) {
        use boxed;
        use mem;
        use ptr;
        use libc;
        use sys_common::thread_local as os;

        extern {
            #[linkage = "extern_weak"]
            static __dso_handle: *mut u8;
            #[linkage = "extern_weak"]
            static __cxa_thread_atexit_impl: *const ();
        }
        if !__cxa_thread_atexit_impl.is_null() {
            type F = unsafe extern fn(dtor: unsafe extern fn(*mut u8),
                                      arg: *mut u8,
                                      dso_handle: *mut u8) -> libc::c_int;
            mem::transmute::<*const (), F>(__cxa_thread_atexit_impl)
            (dtor, t, &__dso_handle as *const _ as *mut _);
            return
        }

        // The fallback implementation uses a vanilla OS-based TLS key to track
        // the list of destructors that need to be run for this thread. The key
        // then has its own destructor which runs all the other destructors.
        //
        // The destructor for DTORS is a little special in that it has a `while`
        // loop to continuously drain the list of registered destructors. It
        // *should* be the case that this loop always terminates because we
        // provide the guarantee that a TLS key cannot be set after it is
        // flagged for destruction.
        static DTORS: os::StaticKey = os::StaticKey::new(Some(run_dtors));
        type List = Vec<(*mut u8, unsafe extern fn(*mut u8))>;
        if DTORS.get().is_null() {
            let v: Box<List> = box Vec::new();
            DTORS.set(boxed::into_raw(v) as *mut u8);
        }
        let list: &mut List = &mut *(DTORS.get() as *mut List);
        list.push((t, dtor));

        unsafe extern fn run_dtors(mut ptr: *mut u8) {
            while !ptr.is_null() {
                let list: Box<List> = Box::from_raw(ptr as *mut List);
                for &(ptr, dtor) in list.iter() {
                    dtor(ptr);
                }
                ptr = DTORS.get();
                DTORS.set(ptr::null_mut());
            }
        }
    }

    // OSX's analog of the above linux function is this _tlv_atexit function.
    // The disassembly of thread_local globals in C++ (at least produced by
    // clang) will have this show up in the output.
    #[cfg(target_os = "macos")]
    unsafe fn register_dtor(t: *mut u8, dtor: unsafe extern fn(*mut u8)) {
        extern {
            fn _tlv_atexit(dtor: unsafe extern fn(*mut u8),
                           arg: *mut u8);
        }
        _tlv_atexit(dtor, t);
    }

    pub unsafe extern fn destroy_value<T>(ptr: *mut u8) {
        let ptr = ptr as *mut Key<T>;
        // Right before we run the user destructor be sure to flag the
        // destructor as running for this thread so calls to `get` will return
        // `None`.
        (*ptr).dtor_running.set(true);
        intrinsics::drop_in_place((*ptr).inner.get());
    }
}

#[cfg(any(not(any(target_os = "macos", target_os = "linux")),
          target_arch = "aarch64",
          no_elf_tls))]
#[doc(hidden)]
mod imp {
    use prelude::v1::*;

    use alloc::boxed;
    use cell::{Cell, UnsafeCell};
    use marker;
    use ptr;
    use sys_common::thread_local::StaticKey as OsStaticKey;

    pub struct Key<T> {
        // OS-TLS key that we'll use to key off.
        os: OsStaticKey,
        marker: marker::PhantomData<Cell<T>>,
    }

    unsafe impl<T> ::marker::Sync for Key<T> { }

    struct Value<T: 'static> {
        key: &'static Key<T>,
        value: UnsafeCell<Option<T>>,
    }

    impl<T: 'static> Key<T> {
        pub const fn new() -> Key<T> {
            Key {
                os: OsStaticKey::new(Some(destroy_value::<T>)),
                marker: marker::PhantomData
            }
        }

        pub unsafe fn get(&'static self) -> Option<&'static UnsafeCell<Option<T>>> {
            let ptr = self.os.get() as *mut Value<T>;
            if !ptr.is_null() {
                if ptr as usize == 1 {
                    return None
                }
                return Some(&(*ptr).value);
            }

            // If the lookup returned null, we haven't initialized our own local
            // copy, so do that now.
            let ptr: Box<Value<T>> = box Value {
                key: self,
                value: UnsafeCell::new(None),
            };
            let ptr = boxed::into_raw(ptr);
            self.os.set(ptr as *mut u8);
            Some(&(*ptr).value)
        }
    }

    pub unsafe extern fn destroy_value<T: 'static>(ptr: *mut u8) {
        // The OS TLS ensures that this key contains a NULL value when this
        // destructor starts to run. We set it back to a sentinel value of 1 to
        // ensure that any future calls to `get` for this thread will return
        // `None`.
        //
        // Note that to prevent an infinite loop we reset it back to null right
        // before we return from the destructor ourselves.
        let ptr = Box::from_raw(ptr as *mut Value<T>);
        let key = ptr.key;
        key.os.set(1 as *mut u8);
        drop(ptr);
        key.os.set(ptr::null_mut());
    }
}

#[cfg(test)]
mod tests {
    use prelude::v1::*;

    use sync::mpsc::{channel, Sender};
    use cell::{Cell, UnsafeCell};
    use super::LocalKeyState;
    use thread;

    struct Foo(Sender<()>);

    impl Drop for Foo {
        fn drop(&mut self) {
            let Foo(ref s) = *self;
            s.send(()).unwrap();
        }
    }

    #[test]
    fn smoke_no_dtor() {
        thread_local!(static FOO: Cell<i32> = Cell::new(1));

        FOO.with(|f| {
            assert_eq!(f.get(), 1);
            f.set(2);
        });
        let (tx, rx) = channel();
        let _t = thread::spawn(move|| {
            FOO.with(|f| {
                assert_eq!(f.get(), 1);
            });
            tx.send(()).unwrap();
        });
        rx.recv().unwrap();

        FOO.with(|f| {
            assert_eq!(f.get(), 2);
        });
    }

    #[test]
    fn states() {
        struct Foo;
        impl Drop for Foo {
            fn drop(&mut self) {
                assert!(FOO.state() == LocalKeyState::Destroyed);
            }
        }
        fn foo() -> Foo {
            assert!(FOO.state() == LocalKeyState::Uninitialized);
            Foo
        }
        thread_local!(static FOO: Foo = foo());

        thread::spawn(|| {
            assert!(FOO.state() == LocalKeyState::Uninitialized);
            FOO.with(|_| {
                assert!(FOO.state() == LocalKeyState::Valid);
            });
            assert!(FOO.state() == LocalKeyState::Valid);
        }).join().ok().unwrap();
    }

    #[test]
    fn smoke_dtor() {
        thread_local!(static FOO: UnsafeCell<Option<Foo>> = UnsafeCell::new(None));

        let (tx, rx) = channel();
        let _t = thread::spawn(move|| unsafe {
            let mut tx = Some(tx);
            FOO.with(|f| {
                *f.get() = Some(Foo(tx.take().unwrap()));
            });
        });
        rx.recv().unwrap();
    }

    #[test]
    fn circular() {
        struct S1;
        struct S2;
        thread_local!(static K1: UnsafeCell<Option<S1>> = UnsafeCell::new(None));
        thread_local!(static K2: UnsafeCell<Option<S2>> = UnsafeCell::new(None));
        static mut HITS: u32 = 0;

        impl Drop for S1 {
            fn drop(&mut self) {
                unsafe {
                    HITS += 1;
                    if K2.state() == LocalKeyState::Destroyed {
                        assert_eq!(HITS, 3);
                    } else {
                        if HITS == 1 {
                            K2.with(|s| *s.get() = Some(S2));
                        } else {
                            assert_eq!(HITS, 3);
                        }
                    }
                }
            }
        }
        impl Drop for S2 {
            fn drop(&mut self) {
                unsafe {
                    HITS += 1;
                    assert!(K1.state() != LocalKeyState::Destroyed);
                    assert_eq!(HITS, 2);
                    K1.with(|s| *s.get() = Some(S1));
                }
            }
        }

        thread::spawn(move|| {
            drop(S1);
        }).join().ok().unwrap();
    }

    #[test]
    fn self_referential() {
        struct S1;
        thread_local!(static K1: UnsafeCell<Option<S1>> = UnsafeCell::new(None));

        impl Drop for S1 {
            fn drop(&mut self) {
                assert!(K1.state() == LocalKeyState::Destroyed);
            }
        }

        thread::spawn(move|| unsafe {
            K1.with(|s| *s.get() = Some(S1));
        }).join().ok().unwrap();
    }

    #[test]
    fn dtors_in_dtors_in_dtors() {
        struct S1(Sender<()>);
        thread_local!(static K1: UnsafeCell<Option<S1>> = UnsafeCell::new(None));
        thread_local!(static K2: UnsafeCell<Option<Foo>> = UnsafeCell::new(None));

        impl Drop for S1 {
            fn drop(&mut self) {
                let S1(ref tx) = *self;
                unsafe {
                    if K2.state() != LocalKeyState::Destroyed {
                        K2.with(|s| *s.get() = Some(Foo(tx.clone())));
                    }
                }
            }
        }

        let (tx, rx) = channel();
        let _t = thread::spawn(move|| unsafe {
            let mut tx = Some(tx);
            K1.with(|s| *s.get() = Some(S1(tx.take().unwrap())));
        });
        rx.recv().unwrap();
    }
}

#[cfg(test)]
mod dynamic_tests {
    use prelude::v1::*;

    use cell::RefCell;
    use collections::HashMap;

    #[test]
    fn smoke() {
        fn square(i: i32) -> i32 { i * i }
        thread_local!(static FOO: i32 = square(3));

        FOO.with(|f| {
            assert_eq!(*f, 9);
        });
    }

    #[test]
    fn hashmap() {
        fn map() -> RefCell<HashMap<i32, i32>> {
            let mut m = HashMap::new();
            m.insert(1, 2);
            RefCell::new(m)
        }
        thread_local!(static FOO: RefCell<HashMap<i32, i32>> = map());

        FOO.with(|map| {
            assert_eq!(map.borrow()[&1], 2);
        });
    }

    #[test]
    fn refcell_vec() {
        thread_local!(static FOO: RefCell<Vec<u32>> = RefCell::new(vec![1, 2, 3]));

        FOO.with(|vec| {
            assert_eq!(vec.borrow().len(), 3);
            vec.borrow_mut().push(4);
            assert_eq!(vec.borrow()[3], 4);
        });
    }
}
