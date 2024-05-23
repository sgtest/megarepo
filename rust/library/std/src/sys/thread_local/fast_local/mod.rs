//! Thread local support for platforms with native TLS.
//!
//! To achieve the best performance, we choose from four different types for
//! the TLS variable, depending from the method of initialization used (`const`
//! or lazy) and the drop requirements of the stored type:
//!
//! |         | `Drop`               | `!Drop`             |
//! |--------:|:--------------------:|:-------------------:|
//! | `const` | `EagerStorage<T>`    | `T`                 |
//! | lazy    | `LazyStorage<T, ()>` | `LazyStorage<T, !>` |
//!
//! For `const` initialization and `!Drop` types, we simply use `T` directly,
//! but for other situations, we implement a state machine to handle
//! initialization of the variable and its destructor and destruction.
//! Upon accessing the TLS variable, the current state is compared:
//!
//! 1. If the state is `Initial`, initialize the storage, transition the state
//!    to `Alive` and (if applicable) register the destructor, and return a
//!    reference to the value.
//! 2. If the state is `Alive`, initialization was previously completed, so
//!    return a reference to the value.
//! 3. If the state is `Destroyed`, the destructor has been run already, so
//!    return [`None`].
//!
//! The TLS destructor sets the state to `Destroyed` and drops the current value.
//!
//! To simplify the code, we make `LazyStorage` generic over the destroyed state
//! and use the `!` type (never type) as type parameter for `!Drop` types. This
//! eliminates the `Destroyed` state for these values, which can allow more niche
//! optimizations to occur for the `State` enum. For `Drop` types, `()` is used.

#![deny(unsafe_op_in_unsafe_fn)]

mod eager;
mod lazy;

pub use eager::Storage as EagerStorage;
pub use lazy::Storage as LazyStorage;

#[doc(hidden)]
#[allow_internal_unstable(
    thread_local_internals,
    cfg_target_thread_local,
    thread_local,
    never_type
)]
#[allow_internal_unsafe]
#[unstable(feature = "thread_local_internals", issue = "none")]
#[rustc_macro_transparency = "semitransparent"]
pub macro thread_local_inner {
    // used to generate the `LocalKey` value for const-initialized thread locals
    (@key $t:ty, const $init:expr) => {{
        const __INIT: $t = $init;

        #[inline]
        #[deny(unsafe_op_in_unsafe_fn)]
        unsafe fn __getit(
            _init: $crate::option::Option<&mut $crate::option::Option<$t>>,
        ) -> $crate::option::Option<&'static $t> {
            use $crate::thread::local_impl::EagerStorage;
            use $crate::mem::needs_drop;
            use $crate::ptr::addr_of;

            if needs_drop::<$t>() {
                #[thread_local]
                static VAL: EagerStorage<$t> = EagerStorage::new(__INIT);
                unsafe {
                    VAL.get()
                }
            } else {
                #[thread_local]
                static VAL: $t = __INIT;
                unsafe {
                    $crate::option::Option::Some(&*addr_of!(VAL))
                }
            }
        }

        unsafe {
            $crate::thread::LocalKey::new(__getit)
        }
    }},

    // used to generate the `LocalKey` value for `thread_local!`
    (@key $t:ty, $init:expr) => {{
        #[inline]
        fn __init() -> $t {
            $init
        }

        #[inline]
        #[deny(unsafe_op_in_unsafe_fn)]
        unsafe fn __getit(
            init: $crate::option::Option<&mut $crate::option::Option<$t>>,
        ) -> $crate::option::Option<&'static $t> {
            use $crate::thread::local_impl::LazyStorage;
            use $crate::mem::needs_drop;

            if needs_drop::<$t>() {
                #[thread_local]
                static VAL: LazyStorage<$t, ()> = LazyStorage::new();
                unsafe {
                    VAL.get_or_init(init, __init)
                }
            } else {
                #[thread_local]
                static VAL: LazyStorage<$t, !> = LazyStorage::new();
                unsafe {
                    VAL.get_or_init(init, __init)
                }
            }
        }

        unsafe {
            $crate::thread::LocalKey::new(__getit)
        }
    }},
    ($(#[$attr:meta])* $vis:vis $name:ident, $t:ty, $($init:tt)*) => {
        $(#[$attr])* $vis const $name: $crate::thread::LocalKey<$t> =
            $crate::thread::local_impl::thread_local_inner!(@key $t, $($init)*);
    },
}
