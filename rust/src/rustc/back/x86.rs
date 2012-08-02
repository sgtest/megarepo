import driver::session;
import session::sess_os_to_meta_os;
import metadata::loader::meta_section_name;

fn get_target_strs(target_os: session::os) -> target_strs::t {
    return {
        module_asm: ~"",

        meta_sect_name: meta_section_name(sess_os_to_meta_os(target_os)),

        data_layout: alt target_os {
          session::os_macos {
            ~"e-p:32:32:32-i1:8:8-i8:8:8-i16:16:16" +
                ~"-i32:32:32-i64:32:64" +
                ~"-f32:32:32-f64:32:64-v64:64:64" +
                ~"-v128:128:128-a0:0:64-f80:128:128" + ~"-n8:16:32"
          }

          session::os_win32 {
            ~"e-p:32:32-f64:64:64-i64:64:64-f80:32:32-n8:16:32"
          }

          session::os_linux {
            ~"e-p:32:32-f64:32:64-i64:32:64-f80:32:32-n8:16:32"
          }

          session::os_freebsd {
            ~"e-p:32:32-f64:32:64-i64:32:64-f80:32:32-n8:16:32"
          }
        },

        target_triple: alt target_os {
          session::os_macos { ~"i686-apple-darwin" }
          session::os_win32 { ~"i686-pc-mingw32" }
          session::os_linux { ~"i686-unknown-linux-gnu" }
          session::os_freebsd { ~"i686-unknown-freebsd" }
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
