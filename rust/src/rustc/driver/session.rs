
import syntax::{ast, codemap};
import syntax::ast::node_id;
import codemap::span;
import syntax::ast::{int_ty, uint_ty, float_ty};
import syntax::parse::parser::parse_sess;
import util::filesearch;
import back::target_strs;
import middle::lint;

enum os { os_win32, os_macos, os_linux, os_freebsd, }

enum arch { arch_x86, arch_x86_64, arch_arm, }

enum crate_type { bin_crate, lib_crate, unknown_crate, }

type config =
    {os: os,
     arch: arch,
     target_strs: target_strs::t,
     int_type: int_ty,
     uint_type: uint_ty,
     float_type: float_ty};

type options =
    // The crate config requested for the session, which may be combined
    // with additional crate configurations during the compile process
    {crate_type: crate_type,
     static: bool,
     optimize: uint,
     debuginfo: bool,
     extra_debuginfo: bool,
     verify: bool,
     lint_opts: [(lint::option, bool)],
     save_temps: bool,
     stats: bool,
     time_passes: bool,
     count_llvm_insns: bool,
     time_llvm_passes: bool,
     output_type: back::link::output_type,
     addl_lib_search_paths: [str],
     maybe_sysroot: option<str>,
     target_triple: str,
     cfg: ast::crate_cfg,
     test: bool,
     parse_only: bool,
     no_trans: bool,
     no_asm_comments: bool,
     warn_unused_imports: bool};

type crate_metadata = {name: str, data: [u8]};

type session = @{targ_cfg: @config,
                 opts: @options,
                 cstore: metadata::cstore::cstore,
                 parse_sess: parse_sess,
                 codemap: codemap::codemap,
                 // For a library crate, this is always none
                 mut main_fn: option<(node_id, codemap::span)>,
                 span_diagnostic: diagnostic::span_handler,
                 filesearch: filesearch::filesearch,
                 mut building_library: bool,
                 working_dir: str};

impl session for session {
    fn span_fatal(sp: span, msg: str) -> ! {
        self.span_diagnostic.span_fatal(sp, msg)
    }
    fn fatal(msg: str) -> ! {
        self.span_diagnostic.handler().fatal(msg)
    }
    fn span_err(sp: span, msg: str) {
        self.span_diagnostic.span_err(sp, msg)
    }
    fn err(msg: str) {
        self.span_diagnostic.handler().err(msg)
    }
    fn has_errors() -> bool {
        self.span_diagnostic.handler().has_errors()
    }
    fn abort_if_errors() {
        self.span_diagnostic.handler().abort_if_errors()
    }
    fn span_warn(sp: span, msg: str) {
        self.span_diagnostic.span_warn(sp, msg)
    }
    fn warn(msg: str) {
        self.span_diagnostic.handler().warn(msg)
    }
    fn span_note(sp: span, msg: str) {
        self.span_diagnostic.span_note(sp, msg)
    }
    fn note(msg: str) {
        self.span_diagnostic.handler().note(msg)
    }
    fn span_bug(sp: span, msg: str) -> ! {
        self.span_diagnostic.span_bug(sp, msg)
    }
    fn bug(msg: str) -> ! {
        self.span_diagnostic.handler().bug(msg)
    }
    fn span_unimpl(sp: span, msg: str) -> ! {
        self.span_diagnostic.span_unimpl(sp, msg)
    }
    fn unimpl(msg: str) -> ! {
        self.span_diagnostic.handler().unimpl(msg)
    }
    fn next_node_id() -> ast::node_id {
        ret syntax::parse::parser::next_node_id(self.parse_sess);
    }
    fn diagnostic() -> diagnostic::span_handler {
        self.span_diagnostic
    }
}

// Seems out of place, but it uses session, so I'm putting it here
fn expect<T: copy>(sess: session, opt: option<T>, msg: fn() -> str) -> T {
    alt opt {
       some(t) { t }
       none { sess.bug(msg()); }
    }
}

fn building_library(req_crate_type: crate_type, crate: @ast::crate,
                    testing: bool) -> bool {
    alt req_crate_type {
      bin_crate { false }
      lib_crate { true }
      unknown_crate {
        if testing {
            false
        } else {
            alt front::attr::get_meta_item_value_str_by_name(
                crate.node.attrs,
                "crate_type") {
              option::some("lib") { true }
              _ { false }
            }
        }
      }
    }
}

#[cfg(test)]
mod test {
    import syntax::ast_util;

    fn make_crate_type_attr(t: str) -> ast::attribute {
        ast_util::respan(ast_util::dummy_sp(), {
            style: ast::attr_outer,
            value: ast_util::respan(ast_util::dummy_sp(),
                ast::meta_name_value(
                    "crate_type",
                    ast_util::respan(ast_util::dummy_sp(),
                                     ast::lit_str(t))))
        })
    }

    fn make_crate(with_bin: bool, with_lib: bool) -> @ast::crate {
        let mut attrs = [];
        if with_bin { attrs += [make_crate_type_attr("bin")]; }
        if with_lib { attrs += [make_crate_type_attr("lib")]; }
        @ast_util::respan(ast_util::dummy_sp(), {
            directives: [],
            module: {view_items: [], items: []},
            attrs: attrs,
            config: []
        })
    }

    #[test]
    fn bin_crate_type_attr_results_in_bin_output() {
        let crate = make_crate(true, false);
        assert !building_library(unknown_crate, crate, false);
    }

    #[test]
    fn lib_crate_type_attr_results_in_lib_output() {
        let crate = make_crate(false, true);
        assert building_library(unknown_crate, crate, false);
    }

    #[test]
    fn bin_option_overrides_lib_crate_type() {
        let crate = make_crate(false, true);
        assert !building_library(bin_crate, crate, false);
    }

    #[test]
    fn lib_option_overrides_bin_crate_type() {
        let crate = make_crate(true, false);
        assert building_library(lib_crate, crate, false);
    }

    #[test]
    fn bin_crate_type_is_default() {
        let crate = make_crate(false, false);
        assert !building_library(unknown_crate, crate, false);
    }

    #[test]
    fn test_option_overrides_lib_crate_type() {
        let crate = make_crate(false, true);
        assert !building_library(unknown_crate, crate, true);
    }

    #[test]
    fn test_option_does_not_override_requested_lib_type() {
        let crate = make_crate(false, false);
        assert building_library(lib_crate, crate, true);
    }
}

// Local Variables:
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
