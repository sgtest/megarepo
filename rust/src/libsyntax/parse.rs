//! The main parser interface

export parse_sess;
export new_parse_sess, new_parse_sess_special_handler;
export next_node_id;
export new_parser_from_file, new_parser_etc_from_file;
export new_parser_from_source_str;
export new_parser_from_tt;
export parse_crate_from_file, parse_crate_from_crate_file;
export parse_crate_from_source_str;
export parse_expr_from_source_str, parse_item_from_source_str;
export parse_stmt_from_source_str;
export parse_from_source_str;

import parser::parser;
import attr::parser_attr;
import common::parser_common;
import ast::node_id;
import util::interner;
import diagnostic::{span_handler, mk_span_handler, mk_handler, emitter};
import lexer::{reader, string_reader};
import parse::token::{ident_interner, mk_ident_interner};

type parse_sess = @{
    cm: codemap::codemap,
    mut next_id: node_id,
    span_diagnostic: span_handler,
    interner: ident_interner,
    // these two must be kept up to date
    mut chpos: uint,
    mut byte_pos: uint
};

fn new_parse_sess(demitter: option<emitter>) -> parse_sess {
    let cm = codemap::new_codemap();
    return @{cm: cm,
             mut next_id: 1,
             span_diagnostic: mk_span_handler(mk_handler(demitter), cm),
             interner: mk_ident_interner(),
             mut chpos: 0u, mut byte_pos: 0u};
}

fn new_parse_sess_special_handler(sh: span_handler, cm: codemap::codemap)
    -> parse_sess {
    return @{cm: cm,
             mut next_id: 1,
             span_diagnostic: sh,
             interner: mk_ident_interner(),
             mut chpos: 0u, mut byte_pos: 0u};
}

fn parse_crate_from_file(input: ~str, cfg: ast::crate_cfg,
                         sess: parse_sess) -> @ast::crate {
    if str::ends_with(input, ~".rc") {
        parse_crate_from_crate_file(input, cfg, sess)
    } else if str::ends_with(input, ~".rs") {
        parse_crate_from_source_file(input, cfg, sess)
    } else {
        sess.span_diagnostic.handler().fatal(~"unknown input file type: " +
                                             input)
    }
}

fn parse_crate_from_crate_file(input: ~str, cfg: ast::crate_cfg,
                               sess: parse_sess) -> @ast::crate {
    let (p, rdr) = new_parser_etc_from_file(sess, cfg, input,
                                            parser::CRATE_FILE);
    let lo = p.span.lo;
    let prefix = path::dirname(input);
    let leading_attrs = p.parse_inner_attrs_and_next();
    let { inner: crate_attrs, next: first_cdir_attr } = leading_attrs;
    let cdirs = p.parse_crate_directives(token::EOF, first_cdir_attr);
    sess.chpos = rdr.chpos;
    sess.byte_pos = sess.byte_pos + rdr.pos;
    let cx = @{sess: sess, cfg: /* FIXME (#2543) */ copy p.cfg};
    let (companionmod, _) = path::splitext(path::basename(input));
    let (m, attrs) = eval::eval_crate_directives_to_mod(
        cx, cdirs, prefix, option::some(companionmod));
    let mut hi = p.span.hi;
    p.expect(token::EOF);
    return @ast_util::respan(ast_util::mk_sp(lo, hi),
                          {directives: cdirs,
                           module: m,
                           attrs: vec::append(crate_attrs, attrs),
                           config: /* FIXME (#2543) */ copy p.cfg});
}

fn parse_crate_from_source_file(input: ~str, cfg: ast::crate_cfg,
                                sess: parse_sess) -> @ast::crate {
    let (p, rdr) = new_parser_etc_from_file(sess, cfg, input,
                                            parser::SOURCE_FILE);
    let r = p.parse_crate_mod(cfg);
    sess.chpos = rdr.chpos;
    sess.byte_pos = sess.byte_pos + rdr.pos;
    return r;
}

fn parse_crate_from_source_str(name: ~str, source: @~str, cfg: ast::crate_cfg,
                               sess: parse_sess) -> @ast::crate {
    let (p, rdr) = new_parser_etc_from_source_str(sess, cfg, name,
                                                  codemap::fss_none, source);
    let r = p.parse_crate_mod(cfg);
    sess.chpos = rdr.chpos;
    sess.byte_pos = sess.byte_pos + rdr.pos;
    return r;
}

fn parse_expr_from_source_str(name: ~str, source: @~str, cfg: ast::crate_cfg,
                              sess: parse_sess) -> @ast::expr {
    let (p, rdr) = new_parser_etc_from_source_str(sess, cfg, name,
                                                  codemap::fss_none, source);
    let r = p.parse_expr();
    sess.chpos = rdr.chpos;
    sess.byte_pos = sess.byte_pos + rdr.pos;
    return r;
}

fn parse_item_from_source_str(name: ~str, source: @~str, cfg: ast::crate_cfg,
                              +attrs: ~[ast::attribute],
                              sess: parse_sess) -> option<@ast::item> {
    let (p, rdr) = new_parser_etc_from_source_str(sess, cfg, name,
                                                  codemap::fss_none, source);
    let r = p.parse_item(attrs);
    sess.chpos = rdr.chpos;
    sess.byte_pos = sess.byte_pos + rdr.pos;
    return r;
}

fn parse_stmt_from_source_str(name: ~str, source: @~str, cfg: ast::crate_cfg,
                              +attrs: ~[ast::attribute],
                              sess: parse_sess) -> @ast::stmt {
    let (p, rdr) = new_parser_etc_from_source_str(sess, cfg, name,
                                                  codemap::fss_none, source);
    let r = p.parse_stmt(attrs);
    sess.chpos = rdr.chpos;
    sess.byte_pos = sess.byte_pos + rdr.pos;
    return r;
}

fn parse_from_source_str<T>(f: fn (p: parser) -> T,
                            name: ~str, ss: codemap::file_substr,
                            source: @~str, cfg: ast::crate_cfg,
                            sess: parse_sess)
    -> T
{
    let (p, rdr) = new_parser_etc_from_source_str(sess, cfg, name, ss,
                                                  source);
    let r = f(p);
    if !p.reader.is_eof() {
        p.reader.fatal(~"expected end-of-string");
    }
    sess.chpos = rdr.chpos;
    sess.byte_pos = sess.byte_pos + rdr.pos;
    return r;
}

fn next_node_id(sess: parse_sess) -> node_id {
    let rv = sess.next_id;
    sess.next_id += 1;
    // ID 0 is reserved for the crate and doesn't actually exist in the AST
    assert rv != 0;
    return rv;
}

fn new_parser_etc_from_source_str(sess: parse_sess, cfg: ast::crate_cfg,
                                  +name: ~str, +ss: codemap::file_substr,
                                  source: @~str) -> (parser, string_reader) {
    let ftype = parser::SOURCE_FILE;
    let filemap = codemap::new_filemap_w_substr
        (name, ss, source, sess.chpos, sess.byte_pos);
    sess.cm.files.push(filemap);
    let srdr = lexer::new_string_reader(sess.span_diagnostic, filemap,
                                        sess.interner);
    return (parser(sess, cfg, srdr as reader, ftype), srdr);
}

fn new_parser_from_source_str(sess: parse_sess, cfg: ast::crate_cfg,
                              +name: ~str, +ss: codemap::file_substr,
                              source: @~str) -> parser {
    let (p, _) = new_parser_etc_from_source_str(sess, cfg, name, ss, source);
    return p;
}


fn new_parser_etc_from_file(sess: parse_sess, cfg: ast::crate_cfg,
                            +path: ~str, ftype: parser::file_type) ->
   (parser, string_reader) {
    let res = io::read_whole_file_str(path);
    match res {
      result::ok(_) => { /* Continue. */ }
      result::err(e) => sess.span_diagnostic.handler().fatal(e)
    }
    let src = @result::unwrap(res);
    let filemap = codemap::new_filemap(path, src, sess.chpos, sess.byte_pos);
    sess.cm.files.push(filemap);
    let srdr = lexer::new_string_reader(sess.span_diagnostic, filemap,
                                        sess.interner);
    return (parser(sess, cfg, srdr as reader, ftype), srdr);
}

fn new_parser_from_file(sess: parse_sess, cfg: ast::crate_cfg, +path: ~str,
                        ftype: parser::file_type) -> parser {
    let (p, _) = new_parser_etc_from_file(sess, cfg, path, ftype);
    return p;
}

fn new_parser_from_tt(sess: parse_sess, cfg: ast::crate_cfg,
                      tt: ~[ast::token_tree]) -> parser {
    let trdr = lexer::new_tt_reader(sess.span_diagnostic, sess.interner,
                                    none, tt);
    return parser(sess, cfg, trdr as reader, parser::SOURCE_FILE)
}
