// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Lints in the Rust compiler.
//!
//! This currently only contains the definitions and implementations
//! of most of the lints that `rustc` supports directly, it does not
//! contain the infrastructure for defining/registering lints. That is
//! available in `rustc::lint` and `rustc_plugin` respectively.
//!
//! # Note
//!
//! This API is completely unstable and subject to change.

#![crate_name = "rustc_lint"]
#![unstable(feature = "rustc_private", issue = "27812")]
#![crate_type = "dylib"]
#![crate_type = "rlib"]
#![doc(html_logo_url = "https://www.rust-lang.org/logos/rust-logo-128x128-blk-v2.png",
      html_favicon_url = "https://doc.rust-lang.org/favicon.ico",
      html_root_url = "https://doc.rust-lang.org/nightly/")]
#![cfg_attr(not(stage0), deny(warnings))]

#![cfg_attr(test, feature(test))]
#![feature(box_patterns)]
#![feature(box_syntax)]
#![feature(quote)]
#![feature(rustc_diagnostic_macros)]
#![feature(rustc_private)]
#![feature(slice_patterns)]
#![feature(staged_api)]

#[macro_use]
extern crate syntax;
#[macro_use]
extern crate rustc;
#[macro_use]
extern crate log;
extern crate rustc_back;
extern crate rustc_const_eval;
extern crate syntax_pos;

pub use rustc::lint as lint;
pub use rustc::middle as middle;
pub use rustc::session as session;
pub use rustc::util as util;

use session::Session;
use lint::LintId;
use lint::FutureIncompatibleInfo;

mod bad_style;
mod builtin;
mod types;
mod unused;

use bad_style::*;
use builtin::*;
use types::*;
use unused::*;

/// Tell the `LintStore` about all the built-in lints (the ones
/// defined in this crate and the ones defined in
/// `rustc::lint::builtin`).
pub fn register_builtins(store: &mut lint::LintStore, sess: Option<&Session>) {
    macro_rules! add_builtin {
        ($sess:ident, $($name:ident),*,) => (
            {$(
                store.register_late_pass($sess, false, box $name);
                )*}
            )
    }

    macro_rules! add_early_builtin {
        ($sess:ident, $($name:ident),*,) => (
            {$(
                store.register_early_pass($sess, false, box $name);
                )*}
            )
    }

    macro_rules! add_builtin_with_new {
        ($sess:ident, $($name:ident),*,) => (
            {$(
                store.register_late_pass($sess, false, box $name::new());
                )*}
            )
    }

    macro_rules! add_lint_group {
        ($sess:ident, $name:expr, $($lint:ident),*) => (
            store.register_group($sess, false, $name, vec![$(LintId::of($lint)),*]);
            )
    }

    add_early_builtin!(sess,
                       UnusedParens,
                       );

    add_builtin!(sess,
                 HardwiredLints,
                 WhileTrue,
                 ImproperCTypes,
                 VariantSizeDifferences,
                 BoxPointers,
                 UnusedAttributes,
                 PathStatements,
                 UnusedResults,
                 NonCamelCaseTypes,
                 NonSnakeCase,
                 NonUpperCaseGlobals,
                 UnusedImportBraces,
                 NonShorthandFieldPatterns,
                 UnusedUnsafe,
                 UnsafeCode,
                 UnusedMut,
                 UnusedAllocation,
                 MissingCopyImplementations,
                 UnstableFeatures,
                 UnconditionalRecursion,
                 InvalidNoMangleItems,
                 PluginAsLibrary,
                 DropWithReprExtern,
                 MutableTransmutes,
                 );

    add_builtin_with_new!(sess,
                          Deprecated,
                          TypeLimits,
                          MissingDoc,
                          MissingDebugImplementations,
                          );

    add_lint_group!(sess, "bad_style",
                    NON_CAMEL_CASE_TYPES, NON_SNAKE_CASE, NON_UPPER_CASE_GLOBALS);

    add_lint_group!(sess, "unused",
                    UNUSED_IMPORTS, UNUSED_VARIABLES, UNUSED_ASSIGNMENTS, DEAD_CODE,
                    UNUSED_MUT, UNREACHABLE_CODE, UNUSED_MUST_USE,
                    UNUSED_UNSAFE, PATH_STATEMENTS, UNUSED_ATTRIBUTES);

    // Guidelines for creating a future incompatibility lint:
    //
    // - Create a lint defaulting to warn as normal, with ideally the same error
    //   message you would normally give
    // - Add a suitable reference, typically an RFC or tracking issue. Go ahead
    //   and include the full URL.
    // - Later, change lint to error
    // - Eventually, remove lint
    store.register_future_incompatible(sess, vec![
        FutureIncompatibleInfo {
            id: LintId::of(PRIVATE_IN_PUBLIC),
            reference: "issue #34537 <https://github.com/rust-lang/rust/issues/34537>",
        },
        FutureIncompatibleInfo {
            id: LintId::of(INACCESSIBLE_EXTERN_CRATE),
            reference: "PR 31362 <https://github.com/rust-lang/rust/pull/31362>",
        },
        FutureIncompatibleInfo {
            id: LintId::of(INVALID_TYPE_PARAM_DEFAULT),
            reference: "PR 30724 <https://github.com/rust-lang/rust/pull/30724>",
        },
        FutureIncompatibleInfo {
            id: LintId::of(SUPER_OR_SELF_IN_GLOBAL_PATH),
            reference: "PR #32403 <https://github.com/rust-lang/rust/pull/32403>",
        },
        FutureIncompatibleInfo {
            id: LintId::of(MATCH_OF_UNIT_VARIANT_VIA_PAREN_DOTDOT),
            reference: "RFC 218 <https://github.com/rust-lang/rfcs/blob/\
                        master/text/0218-empty-struct-with-braces.md>",
        },
        FutureIncompatibleInfo {
            id: LintId::of(TRANSMUTE_FROM_FN_ITEM_TYPES),
            reference: "issue #19925 <https://github.com/rust-lang/rust/issues/19925>",
        },
        FutureIncompatibleInfo {
            id: LintId::of(OVERLAPPING_INHERENT_IMPLS),
            reference: "issue #22889 <https://github.com/rust-lang/rust/issues/22889>",
        },
        FutureIncompatibleInfo {
            id: LintId::of(ILLEGAL_FLOATING_POINT_CONSTANT_PATTERN),
            reference: "RFC 1445 <https://github.com/rust-lang/rfcs/pull/1445>",
        },
        FutureIncompatibleInfo {
            id: LintId::of(ILLEGAL_STRUCT_OR_ENUM_CONSTANT_PATTERN),
            reference: "RFC 1445 <https://github.com/rust-lang/rfcs/pull/1445>",
        },
        FutureIncompatibleInfo {
            id: LintId::of(UNSIZED_IN_TUPLE),
            reference: "issue #33242 <https://github.com/rust-lang/rust/issues/33242>",
        },
        FutureIncompatibleInfo {
            id: LintId::of(OBJECT_UNSAFE_FRAGMENT),
            reference: "issue #33243 <https://github.com/rust-lang/rust/issues/33243>",
        },
        FutureIncompatibleInfo {
            id: LintId::of(HR_LIFETIME_IN_ASSOC_TYPE),
            reference: "issue #33685 <https://github.com/rust-lang/rust/issues/33685>",
        },
        FutureIncompatibleInfo {
            id: LintId::of(LIFETIME_UNDERSCORE),
            reference: "RFC 1177 <https://github.com/rust-lang/rfcs/pull/1177>",
        },
        ]);

    // Register renamed and removed lints
    store.register_renamed("unknown_features", "unused_features");
    store.register_removed("unsigned_negation", "replaced by negate_unsigned feature gate");
    store.register_removed("negate_unsigned", "cast a signed value instead");
    store.register_removed("raw_pointer_derive", "using derive with raw pointers is ok");
    // This was renamed to raw_pointer_derive, which was then removed,
    // so it is also considered removed
    store.register_removed("raw_pointer_deriving", "using derive with raw pointers is ok");
}
