#![feature(allow_internal_unstable)]
#![feature(bench_black_box)]
#![feature(extend_one)]
#![feature(let_else)]
#![feature(min_specialization)]
#![feature(new_uninit)]
#![feature(step_trait)]
#![feature(stmt_expr_attributes)]
#![feature(test)]

pub mod bit_set;
pub mod interval;
pub mod vec;

pub use rustc_macros::newtype_index;

/// Type size assertion. The first argument is a type and the second argument is its expected size.
#[macro_export]
macro_rules! static_assert_size {
    ($ty:ty, $size:expr) => {
        const _: [(); $size] = [(); ::std::mem::size_of::<$ty>()];
    };
}
