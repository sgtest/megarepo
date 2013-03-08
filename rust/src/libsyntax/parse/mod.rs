// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! The main parser interface


use ast::node_id;
use ast;
use codemap::{span, CodeMap, FileMap, CharPos, BytePos};
use codemap;
use diagnostic::{span_handler, mk_span_handler, mk_handler, Emitter};
use parse::attr::parser_attr;
use parse::lexer::{reader, StringReader};
use parse::parser::Parser;
use parse::token::{ident_interner, mk_ident_interner};

use core::io;
use core::option::{None, Option, Some};
use core::path::Path;
use core::result::{Err, Ok, Result};

pub mod lexer;
pub mod parser;
pub mod token;
pub mod comments;
pub mod attr;


/// Common routines shared by parser mods
pub mod common;

/// Functions dealing with operator precedence
pub mod prec;

/// Routines the parser uses to classify AST nodes
pub mod classify;

/// Reporting obsolete syntax
pub mod obsolete;

pub struct ParseSess {
    cm: @codemap::CodeMap,
    next_id: node_id,
    span_diagnostic: span_handler,
    interner: @ident_interner,
}

pub fn new_parse_sess(demitter: Option<Emitter>) -> @mut ParseSess {
    let cm = @CodeMap::new();
    @mut ParseSess {
        cm: cm,
        next_id: 1,
        span_diagnostic: mk_span_handler(mk_handler(demitter), cm),
        interner: mk_ident_interner(),
    }
}

pub fn new_parse_sess_special_handler(sh: span_handler, cm: @codemap::CodeMap)
    -> @mut ParseSess {
    @mut ParseSess {
        cm: cm,
        next_id: 1,
        span_diagnostic: sh,
        interner: mk_ident_interner(),
    }
}

// a bunch of utility functions of the form parse_<thing>_from_<source>
// where <thing> includes crate, expr, item, stmt, tts, and one that
// uses a HOF to parse anything, and <source> includes file and
// source_str.

// this appears to be the main entry point for rust parsing by
// rustc and crate:
pub fn parse_crate_from_file(
    input: &Path,
    cfg: ast::crate_cfg,
    sess: @mut ParseSess
) -> @ast::crate {
    let p = new_parser_from_file(sess, /*bad*/ copy cfg, input);
    p.parse_crate_mod(/*bad*/ copy cfg)
    // why is there no p.abort_if_errors here?
}

pub fn parse_crate_from_source_str(
    name: ~str,
    source: @~str,
    cfg: ast::crate_cfg,
    sess: @mut ParseSess
) -> @ast::crate {
    let p = new_parser_from_source_str(
        sess,
        /*bad*/ copy cfg,
        /*bad*/ copy name,
        codemap::FssNone,
        source
    );
    maybe_aborted(p.parse_crate_mod(/*bad*/ copy cfg),p)
}

pub fn parse_expr_from_source_str(
    name: ~str,
    source: @~str,
    +cfg: ast::crate_cfg,
    sess: @mut ParseSess
) -> @ast::expr {
    let p = new_parser_from_source_str(
        sess,
        cfg,
        /*bad*/ copy name,
        codemap::FssNone,
        source
    );
    maybe_aborted(p.parse_expr(), p)
}

pub fn parse_item_from_source_str(
    name: ~str,
    source: @~str,
    +cfg: ast::crate_cfg,
    +attrs: ~[ast::attribute],
    sess: @mut ParseSess
) -> Option<@ast::item> {
    let p = new_parser_from_source_str(
        sess,
        cfg,
        /*bad*/ copy name,
        codemap::FssNone,
        source
    );
    maybe_aborted(p.parse_item(attrs),p)
}

pub fn parse_stmt_from_source_str(
    name: ~str,
    source: @~str,
    +cfg: ast::crate_cfg,
    +attrs: ~[ast::attribute],
    sess: @mut ParseSess
) -> @ast::stmt {
    let p = new_parser_from_source_str(
        sess,
        cfg,
        /*bad*/ copy name,
        codemap::FssNone,
        source
    );
    maybe_aborted(p.parse_stmt(attrs),p)
}

pub fn parse_tts_from_source_str(
    name: ~str,
    source: @~str,
    +cfg: ast::crate_cfg,
    sess: @mut ParseSess
) -> ~[ast::token_tree] {
    let p = new_parser_from_source_str(
        sess,
        cfg,
        /*bad*/ copy name,
        codemap::FssNone,
        source
    );
    *p.quote_depth += 1u;
    maybe_aborted(p.parse_all_token_trees(),p)
}

pub fn parse_from_source_str<T>(
    f: fn (Parser) -> T,
    name: ~str, ss: codemap::FileSubstr,
    source: @~str,
    +cfg: ast::crate_cfg,
    sess: @mut ParseSess
) -> T {
    let p = new_parser_from_source_str(
        sess,
        cfg,
        /*bad*/ copy name,
        /*bad*/ copy ss,
        source
    );
    let r = f(p);
    if !p.reader.is_eof() {
        p.reader.fatal(~"expected end-of-string");
    }
    maybe_aborted(r,p)
}

pub fn next_node_id(sess: @mut ParseSess) -> node_id {
    let rv = sess.next_id;
    sess.next_id += 1;
    // ID 0 is reserved for the crate and doesn't actually exist in the AST
    fail_unless!(rv != 0);
    return rv;
}

pub fn new_parser_from_source_str(
    sess: @mut ParseSess,
    +cfg: ast::crate_cfg,
    +name: ~str,
    +ss: codemap::FileSubstr,
    source: @~str
) -> Parser {
    let filemap = sess.cm.new_filemap_w_substr(name, ss, source);
    let srdr = lexer::new_string_reader(
        copy sess.span_diagnostic,
        filemap,
        sess.interner
    );
    Parser(sess, cfg, srdr as reader)
}

/// Read the entire source file, return a parser
/// that draws from that string
pub fn new_parser_result_from_file(
    sess: @mut ParseSess,
    +cfg: ast::crate_cfg,
    path: &Path
) -> Result<Parser, ~str> {
    match io::read_whole_file_str(path) {
        Ok(src) => {
            let filemap = sess.cm.new_filemap(path.to_str(), @src);
            let srdr = lexer::new_string_reader(
                copy sess.span_diagnostic,
                filemap,
                sess.interner
            );
            Ok(Parser(sess, cfg, srdr as reader))

        }
        Err(e) => Err(e)
    }
}

/// Create a new parser, handling errors as appropriate
/// if the file doesn't exist
pub fn new_parser_from_file(
    sess: @mut ParseSess,
    +cfg: ast::crate_cfg,
    path: &Path
) -> Parser {
    match new_parser_result_from_file(sess, cfg, path) {
        Ok(parser) => parser,
        Err(e) => {
            sess.span_diagnostic.handler().fatal(e)
        }
    }
}

/// Create a new parser based on a span from an existing parser. Handles
/// error messages correctly when the file does not exist.
pub fn new_sub_parser_from_file(
    sess: @mut ParseSess,
    +cfg: ast::crate_cfg,
    path: &Path,
    sp: span
) -> Parser {
    match new_parser_result_from_file(sess, cfg, path) {
        Ok(parser) => parser,
        Err(e) => {
            sess.span_diagnostic.span_fatal(sp, e)
        }
    }
}

pub fn new_parser_from_tts(
    sess: @mut ParseSess,
    +cfg: ast::crate_cfg,
    +tts: ~[ast::token_tree]
) -> Parser {
    let trdr = lexer::new_tt_reader(
        copy sess.span_diagnostic,
        sess.interner,
        None,
        tts
    );
    Parser(sess, cfg, trdr as reader)
}

// abort if necessary
pub fn maybe_aborted<T>(+result : T, p: Parser) -> T {
    p.abort_if_errors();
    result
}



#[cfg(test)]
mod test {
    use super::*;
    use std::serialize::Encodable;
    use std;
    use core::io;
    use core::option::None;
    use core::str;
    use util::testing::*;

    #[test] fn to_json_str (val: Encodable<std::json::Encoder>) -> ~str {
        let bw = @io::BytesWriter();
        val.encode(~std::json::Encoder(bw as io::Writer));
        str::from_bytes(bw.bytes.data)
    }

    #[test] fn alltts () {
        let tts = parse_tts_from_source_str(
            ~"bogofile",
            @~"fn foo (x : int) { x; }",
            ~[],
            new_parse_sess(None));
        check_equal(to_json_str(@tts as Encodable::<std::json::Encoder>),
                    ~"[[\"tt_tok\",[,[\"IDENT\",[\"fn\",false]]]],\
                      [\"tt_tok\",[,[\"IDENT\",[\"foo\",false]]]],\
                      [\"tt_delim\",[[[\"tt_tok\",[,[\"LPAREN\",[]]]],\
                      [\"tt_tok\",[,[\"IDENT\",[\"x\",false]]]],\
                      [\"tt_tok\",[,[\"COLON\",[]]]],\
                      [\"tt_tok\",[,[\"IDENT\",[\"int\",false]]]],\
                      [\"tt_tok\",[,[\"RPAREN\",[]]]]]]],\
                      [\"tt_delim\",[[[\"tt_tok\",[,[\"LBRACE\",[]]]],\
                      [\"tt_tok\",[,[\"IDENT\",[\"x\",false]]]],\
                      [\"tt_tok\",[,[\"SEMI\",[]]]],\
                      [\"tt_tok\",[,[\"RBRACE\",[]]]]]]]]"
                   );
        let ast1 = new_parser_from_tts(new_parse_sess(None),~[],tts)
            .parse_item(~[]);
        let ast2 = parse_item_from_source_str(
            ~"bogofile",
            @~"fn foo (x : int) { x; }",
            ~[],~[],
            new_parse_sess(None));
        check_equal(ast1,ast2);
    }
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
