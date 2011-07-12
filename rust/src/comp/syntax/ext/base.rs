import std::ivec;
import std::option;
import std::map::hashmap;
import driver::session::session;
import codemap::span;
import std::map::new_str_hash;
import codemap;

type syntax_expander = 
    fn(&ext_ctxt, span, &(@ast::expr)[], option::t[str]) -> @ast::expr;
type macro_definer = fn(&ext_ctxt, span, &(@ast::expr)[],
                        option::t[str]) -> tup(str, syntax_extension);

tag syntax_extension {
    normal(syntax_expander);
    macro_defining(macro_definer);
}

// A temporary hard-coded map of methods for expanding syntax extension
// AST nodes into full ASTs
fn syntax_expander_table() -> hashmap[str, syntax_extension] {
    auto syntax_expanders = new_str_hash[syntax_extension]();
    syntax_expanders.insert("fmt", normal(ext::fmt::expand_syntax_ext));
    syntax_expanders.insert("env", normal(ext::env::expand_syntax_ext));
    syntax_expanders.insert("macro",    
                            macro_defining(ext::simplext::add_new_extension));
    ret syntax_expanders;
}

type span_msg_fn = fn(span, str) -> !  ;

type next_id_fn = fn() -> ast::node_id ;


// Provides a limited set of services necessary for syntax extensions
// to do their thing
type ext_ctxt =
    rec(str crate_file_name_hack,
        span_msg_fn span_fatal,
        span_msg_fn span_unimpl,
        next_id_fn next_id);

fn mk_ctxt(&session sess) -> ext_ctxt {
    fn ext_span_fatal_(&session sess, span sp, str msg) -> ! {
        sess.span_err(sp, msg);
        fail;
    }
    auto ext_span_fatal = bind ext_span_fatal_(sess, _, _);
    fn ext_span_unimpl_(&session sess, span sp, str msg) -> ! {
        sess.span_err(sp, "unimplemented " + msg);
        fail;
    }

    // FIXME: Some extensions work by building ASTs with paths to functions
    // they need to call at runtime. As those functions live in the std crate,
    // the paths are prefixed with "std::". Unfortunately, these paths can't
    // work for code called from inside the stdard library, so here we pass
    // the extensions the file name of the crate being compiled so they can
    // use it to guess whether paths should be prepended with "std::". This is
    // super-ugly and needs a better solution.
    auto crate_file_name_hack = sess.get_codemap().files.(0).name;
    auto ext_span_unimpl = bind ext_span_unimpl_(sess, _, _);
    fn ext_next_id_(&session sess) -> ast::node_id {
        ret sess.next_node_id(); // temporary, until bind works better
    }
    auto ext_next_id = bind ext_next_id_(sess);
    ret rec(crate_file_name_hack=crate_file_name_hack,
            span_fatal=ext_span_fatal,
            span_unimpl=ext_span_unimpl,
            next_id=ext_next_id);
}

fn expr_to_str(&ext_ctxt cx, @ast::expr expr, str error) -> str {
    alt (expr.node) {
        case (ast::expr_lit(?l)) {
            alt (l.node) {
                case (ast::lit_str(?s, _)) { ret s; }
                case (_) { cx.span_fatal(l.span, error); }
            }
        }
        case (_) { cx.span_fatal(expr.span, error); }
    }
}

fn expr_to_ident(&ext_ctxt cx, @ast::expr expr, str error) -> ast::ident {
    alt(expr.node) {
        case (ast::expr_path(?p)) {
            if (ivec::len(p.node.types) > 0u 
                    || ivec::len(p.node.idents) != 1u) {
                cx.span_fatal(expr.span, error);
            } else {
                ret p.node.idents.(0);
            }
        }
        case (_) {
            cx.span_fatal(expr.span, error);
        }
    }
}



//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C $RBUILD 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
//
