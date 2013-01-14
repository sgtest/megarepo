// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


use back::target_strs;
use driver::session;
use metadata::loader::meta_section_name;
use session::sess_os_to_meta_os;

fn get_target_strs(target_os: session::os) -> target_strs::t {
    return {
        module_asm: ~"",

        meta_sect_name: meta_section_name(sess_os_to_meta_os(target_os)),

        data_layout: match target_os {
          session::os_macos => {
            ~"e-p:32:32:32-i1:8:8-i8:8:8-i16:16:16" +
                ~"-i32:32:32-i64:32:64" +
                ~"-f32:32:32-f64:32:64-v64:64:64" +
                ~"-v128:128:128-a0:0:64-f80:128:128" + ~"-n8:16:32"
          }

          session::os_win32 => {
            ~"e-p:32:32-f64:64:64-i64:64:64-f80:32:32-n8:16:32"
          }

          session::os_linux => {
            ~"e-p:32:32-f64:32:64-i64:32:64-f80:32:32-n8:16:32"
          }
          session::os_android => {
            ~"e-p:32:32-f64:32:64-i64:32:64-f80:32:32-n8:16:32"
          }

          session::os_freebsd => {
            ~"e-p:32:32-f64:32:64-i64:32:64-f80:32:32-n8:16:32"
          }
        },

        target_triple: match target_os {
          session::os_macos => ~"i686-apple-darwin",
          session::os_win32 => ~"i686-pc-mingw32",
          session::os_linux => ~"i686-unknown-linux-gnu",
          session::os_android => ~"i686-unknown-android-gnu",
          session::os_freebsd => ~"i686-unknown-freebsd"
        },

        cc_args: ~[~"-m32"]
    };
}

//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
//
