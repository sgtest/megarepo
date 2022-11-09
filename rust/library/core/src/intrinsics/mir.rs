//! Rustc internal tooling for hand-writing MIR.
//!
//! If for some reasons you are not writing rustc tests and have found yourself considering using
//! this feature, turn back. This is *exceptionally* unstable. There is no attempt at all to make
//! anything work besides those things which the rustc test suite happened to need. If you make a
//! typo you'll probably ICE. Really, this is not the solution to your problems. Consider instead
//! supporting the [stable MIR project group](https://github.com/rust-lang/project-stable-mir).
//!
//! The documentation for this module describes how to use this feature. If you are interested in
//! hacking on the implementation, most of that documentation lives at
//! `rustc_mir_building/src/build/custom/mod.rs`.
//!
//! Typical usage will look like this:
//!
//! ```rust
//! #![feature(core_intrinsics, custom_mir)]
//!
//! extern crate core;
//! use core::intrinsics::mir::*;
//!
//! #[custom_mir(dialect = "built")]
//! pub fn simple(x: i32) -> i32 {
//!     mir!(
//!         let temp1: i32;
//!         let temp2: _;
//!
//!         {
//!             temp1 = x;
//!             Goto(exit)
//!         }
//!
//!         exit = {
//!             temp2 = Move(temp1);
//!             RET = temp2;
//!             Return()
//!         }
//!     )
//! }
//! ```
//!
//! Hopefully most of this is fairly self-explanatory. Expanding on some notable details:
//!
//!  - The `custom_mir` attribute tells the compiler to treat the function as being custom MIR. This
//!    attribute only works on functions - there is no way to insert custom MIR into the middle of
//!    another function.
//!  - The `dialect` and `phase` parameters indicate which version of MIR you are inserting here.
//!    This will normally be the phase that corresponds to the thing you are trying to test. The
//!    phase can be omitted for dialects that have just one.
//!  - You should define your function signature like you normally would. Externally, this function
//!    can be called like any other function.
//!  - Type inference works - you don't have to spell out the type of all of your locals.
//!
//! For now, all statements and terminators are parsed from nested invocations of the special
//! functions provided in this module. We additionally want to (but do not yet) support more
//! "normal" Rust syntax in places where it makes sense. Also, most kinds of instructions are not
//! supported yet.
//!

#![unstable(
    feature = "custom_mir",
    reason = "MIR is an implementation detail and extremely unstable",
    issue = "none"
)]
#![allow(unused_variables, non_snake_case, missing_debug_implementations)]

/// Type representing basic blocks.
///
/// All terminators will have this type as a return type. It helps achieve some type safety.
pub struct BasicBlock;

macro_rules! define {
    ($name:literal, $($sig:tt)*) => {
        #[rustc_diagnostic_item = $name]
        pub $($sig)* { panic!() }
    }
}

define!("mir_return", fn Return() -> BasicBlock);
define!("mir_goto", fn Goto(destination: BasicBlock) -> BasicBlock);
define!("mir_retag", fn Retag<T>(place: T));
define!("mir_retag_raw", fn RetagRaw<T>(place: T));
define!("mir_move", fn Move<T>(place: T) -> T);

/// Convenience macro for generating custom MIR.
///
/// See the module documentation for syntax details. This macro is not magic - it only transforms
/// your MIR into something that is easier to parse in the compiler.
#[rustc_macro_transparency = "transparent"]
pub macro mir {
    (
        $(let $local_decl:ident $(: $local_decl_ty:ty)? ;)*

        $entry_block:block

        $(
            $block_name:ident = $block:block
        )*
    ) => {{
        // First, we declare all basic blocks.
        $(
            let $block_name: ::core::intrinsics::mir::BasicBlock;
        )*

        {
            // Now all locals
            #[allow(non_snake_case)]
            let RET;
            $(
                let $local_decl $(: $local_decl_ty)? ;
            )*

            {
                // Finally, the contents of the basic blocks
                $entry_block;
                $(
                    $block;
                )*

                RET
            }
        }
    }}
}
