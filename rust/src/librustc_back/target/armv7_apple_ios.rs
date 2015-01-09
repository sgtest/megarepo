// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use target::{Target, TargetOptions};
use super::apple_ios_base::{opts, Arch};

pub fn target() -> Target {
    Target {
        data_layout: "e-p:32:32-f64:32:64-v64:32:64-v128:32:128-a:0:32-n32-S32".to_string(),
        llvm_target: "armv7-apple-ios".to_string(),
        target_endian: "little".to_string(),
        target_pointer_width: "32".to_string(),
        arch: "arm".to_string(),
        target_os: "ios".to_string(),
        options: TargetOptions {
            features: "+v7,+vfp3,+neon".to_string(),
            .. opts(Arch::Armv7)
        }
    }
}
