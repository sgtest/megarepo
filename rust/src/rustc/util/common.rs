import std::map::hashmap;
import syntax::ast;
import ast::{ty, pat};
import syntax::codemap::{span};
import syntax::visit;
import syntax::print;

fn indent<R>(op: fn() -> R) -> R {
    // Use in conjunction with the log post-processor like `src/etc/indenter`
    // to make debug output more readable.
    #debug[">>"];
    let r <- op();
    #debug["<< (Result = %?)", r];
    ret r;
}

resource _indenter(_i: ()) {
    #debug["<<"];
}

fn indenter() -> _indenter {
    #debug[">>"];
    _indenter(())
}

type flag = hashmap<str, ()>;

fn field_expr(f: ast::field) -> @ast::expr { ret f.node.expr; }

fn field_exprs(fields: [ast::field]) -> [@ast::expr] {
    let mut es = [];
    for fields.each {|f| es += [f.node.expr]; }
    ret es;
}

fn has_nonlocal_exits(b: ast::blk) -> bool {
    let has_exits = @mut false;
    fn visit_expr(flag: @mut bool, e: @ast::expr) {
        alt e.node {
          ast::expr_break { *flag = true; }
          ast::expr_cont { *flag = true; }
          _ { }
        }
    }
    let v =
        visit::mk_simple_visitor(@{visit_expr: bind visit_expr(has_exits, _)
                                      with *visit::default_simple_visitor()});
    visit::visit_block(b, (), v);
    ret *has_exits;
}

/* FIXME: copy/paste, yuck */
fn may_break(b: ast::blk) -> bool {
    let has_exits = @mut false;
    fn visit_expr(flag: @mut bool, e: @ast::expr) {
        alt e.node {
          ast::expr_break { *flag = true; }
          _ { }
        }
    }
    let v =
        visit::mk_simple_visitor(@{visit_expr: bind visit_expr(has_exits, _)
                                      with *visit::default_simple_visitor()});
    visit::visit_block(b, (), v);
    ret *has_exits;
}

fn local_rhs_span(l: @ast::local, def: span) -> span {
    alt l.node.init { some(i) { ret i.expr.span; } _ { ret def; } }
}

fn is_main_name(path: syntax::ast_map::path) -> bool {
    // FIXME: path should be a constrained type, so we know
    // the call to last doesn't fail
    vec::last(path) == syntax::ast_map::path_name(@"main")
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
