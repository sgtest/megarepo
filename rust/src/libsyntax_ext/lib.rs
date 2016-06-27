// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Syntax extensions in the Rust compiler.

#![crate_name = "syntax_ext"]
#![unstable(feature = "rustc_private", issue = "27812")]
#![crate_type = "dylib"]
#![crate_type = "rlib"]
#![doc(html_logo_url = "https://www.rust-lang.org/logos/rust-logo-128x128-blk-v2.png",
       html_favicon_url = "https://doc.rust-lang.org/favicon.ico",
       html_root_url = "https://doc.rust-lang.org/nightly/")]
#![cfg_attr(not(stage0), deny(warnings))]

#![feature(rustc_private)]
#![feature(staged_api)]

extern crate fmt_macros;
#[macro_use] extern crate log;
#[macro_use]
extern crate syntax;
extern crate syntax_pos;
extern crate rustc_errors as errors;

use syntax::ext::base::{MacroExpanderFn, NormalTT};
use syntax::ext::base::{SyntaxEnv, SyntaxExtension};
use syntax::parse::token::intern;


mod asm;
mod cfg;
mod concat;
mod concat_idents;
mod env;
mod format;
mod log_syntax;
mod trace_macros;

// for custom_derive
pub mod deriving;

pub fn register_builtins(env: &mut SyntaxEnv) {
    // utility function to simplify creating NormalTT syntax extensions
    fn builtin_normal_expander(f: MacroExpanderFn) -> SyntaxExtension {
        NormalTT(Box::new(f), None, false)
    }

    env.insert(intern("asm"),
               builtin_normal_expander(asm::expand_asm));
    env.insert(intern("cfg"),
               builtin_normal_expander(cfg::expand_cfg));
    env.insert(intern("concat"),
               builtin_normal_expander(concat::expand_syntax_ext));
    env.insert(intern("concat_idents"),
               builtin_normal_expander(concat_idents::expand_syntax_ext));
    env.insert(intern("env"),
               builtin_normal_expander(env::expand_env));
    env.insert(intern("option_env"),
               builtin_normal_expander(env::expand_option_env));
    env.insert(intern("format_args"),
               // format_args uses `unstable` things internally.
               NormalTT(Box::new(format::expand_format_args), None, true));
    env.insert(intern("log_syntax"),
               builtin_normal_expander(log_syntax::expand_syntax_ext));
    env.insert(intern("trace_macros"),
               builtin_normal_expander(trace_macros::expand_trace_macros));

    deriving::register_all(env);
}
