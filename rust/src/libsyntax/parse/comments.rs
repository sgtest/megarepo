// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::prelude::*;

use ast;
use codemap::{BytePos, CharPos, CodeMap, FileMap};
use diagnostic;
use parse::lexer::{is_whitespace, get_str_from, reader};
use parse::lexer::{string_reader, bump, is_eof, nextch};
use parse::lexer;
use parse::token;
use parse;
use util::interner;

use core::cmp;
use core::io::ReaderUtil;
use core::io;
use core::str;
use core::uint;
use core::vec;

export cmnt;
export lit;
export cmnt_style;
export gather_comments_and_literals;
export is_doc_comment, doc_comment_style, strip_doc_comment_decoration;
export isolated, trailing, mixed, blank_line;

enum cmnt_style {
    isolated, // No code on either side of each line of the comment
    trailing, // Code exists to the left of the comment
    mixed, // Code before /* foo */ and after the comment
    blank_line, // Just a manual blank line "\n\n", for layout
}

impl cmnt_style : cmp::Eq {
    pure fn eq(&self, other: &cmnt_style) -> bool {
        ((*self) as uint) == ((*other) as uint)
    }
    pure fn ne(&self, other: &cmnt_style) -> bool {
        ((*self) as uint) != ((*other) as uint)
    }
}

type cmnt = {style: cmnt_style, lines: ~[~str], pos: BytePos};

fn is_doc_comment(s: ~str) -> bool {
    s.starts_with(~"///") ||
    s.starts_with(~"//!") ||
    s.starts_with(~"/**") ||
    s.starts_with(~"/*!")
}

fn doc_comment_style(comment: ~str) -> ast::attr_style {
    assert is_doc_comment(comment);
    if comment.starts_with(~"//!") || comment.starts_with(~"/*!") {
        ast::attr_inner
    } else {
        ast::attr_outer
    }
}

fn strip_doc_comment_decoration(comment: ~str) -> ~str {

    /// remove whitespace-only lines from the start/end of lines
    fn vertical_trim(lines: ~[~str]) -> ~[~str] {
        let mut i = 0u, j = lines.len();
        while i < j && lines[i].trim().is_empty() {
            i += 1u;
        }
        while j > i && lines[j - 1u].trim().is_empty() {
            j -= 1u;
        }
        return lines.slice(i, j);
    }

    // drop leftmost columns that contain only values in chars
    fn block_trim(lines: ~[~str], chars: ~str, max: Option<uint>) -> ~[~str] {

        let mut i = max.get_or_default(uint::max_value);
        for lines.each |line| {
            if line.trim().is_empty() {
                loop;
            }
            for line.each_chari |j, c| {
                if j >= i {
                    break;
                }
                if !chars.contains_char(c) {
                    i = j;
                    break;
                }
            }
        }

        return do lines.map |line| {
            let chars = str::chars(*line);
            if i > chars.len() {
                ~""
            } else {
                str::from_chars(chars.slice(i, chars.len()))
            }
        };
    }

    if comment.starts_with(~"//") {
        return comment.slice(3u, comment.len()).trim();
    }

    if comment.starts_with(~"/*") {
        let lines = str::lines_any(comment.slice(3u, comment.len() - 2u));
        let lines = vertical_trim(lines);
        let lines = block_trim(lines, ~"\t ", None);
        let lines = block_trim(lines, ~"*", Some(1u));
        let lines = block_trim(lines, ~"\t ", None);
        return str::connect(lines, ~"\n");
    }

    fail ~"not a doc-comment: " + comment;
}

fn read_to_eol(rdr: string_reader) -> ~str {
    let mut val = ~"";
    while rdr.curr != '\n' && !is_eof(rdr) {
        str::push_char(&mut val, rdr.curr);
        bump(rdr);
    }
    if rdr.curr == '\n' { bump(rdr); }
    return val;
}

fn read_one_line_comment(rdr: string_reader) -> ~str {
    let val = read_to_eol(rdr);
    assert ((val[0] == '/' as u8 && val[1] == '/' as u8) ||
            (val[0] == '#' as u8 && val[1] == '!' as u8));
    return val;
}

fn consume_non_eol_whitespace(rdr: string_reader) {
    while is_whitespace(rdr.curr) && rdr.curr != '\n' && !is_eof(rdr) {
        bump(rdr);
    }
}

fn push_blank_line_comment(rdr: string_reader, comments: &mut ~[cmnt]) {
    debug!(">>> blank-line comment");
    let v: ~[~str] = ~[];
    comments.push({style: blank_line, lines: v, pos: rdr.last_pos});
}

fn consume_whitespace_counting_blank_lines(rdr: string_reader,
                                           comments: &mut ~[cmnt]) {
    while is_whitespace(rdr.curr) && !is_eof(rdr) {
        if rdr.col == CharPos(0u) && rdr.curr == '\n' {
            push_blank_line_comment(rdr, &mut *comments);
        }
        bump(rdr);
    }
}


fn read_shebang_comment(rdr: string_reader, code_to_the_left: bool,
                                            comments: &mut ~[cmnt]) {
    debug!(">>> shebang comment");
    let p = rdr.last_pos;
    debug!("<<< shebang comment");
    comments.push({
        style: if code_to_the_left { trailing } else { isolated },
        lines: ~[read_one_line_comment(rdr)],
        pos: p
    });
}

fn read_line_comments(rdr: string_reader, code_to_the_left: bool,
                                          comments: &mut ~[cmnt]) {
    debug!(">>> line comments");
    let p = rdr.last_pos;
    let mut lines: ~[~str] = ~[];
    while rdr.curr == '/' && nextch(rdr) == '/' {
        let line = read_one_line_comment(rdr);
        log(debug, line);
        if is_doc_comment(line) { // doc-comments are not put in comments
            break;
        }
        lines.push(line);
        consume_non_eol_whitespace(rdr);
    }
    debug!("<<< line comments");
    if !lines.is_empty() {
        comments.push({
            style: if code_to_the_left { trailing } else { isolated },
            lines: lines,
            pos: p
        });
    }
}

// FIXME #3961: This is not the right way to convert string byte
// offsets to characters.
fn all_whitespace(s: ~str, begin: uint, end: uint) -> bool {
    let mut i: uint = begin;
    while i != end {
        if !is_whitespace(s[i] as char) { return false; } i += 1u;
    }
    return true;
}

fn trim_whitespace_prefix_and_push_line(lines: &mut ~[~str],
                                        s: ~str, col: CharPos) {
    let mut s1;
    let len = str::len(s);
    // FIXME #3961: Doing bytewise comparison and slicing with CharPos
    let col = col.to_uint();
    if all_whitespace(s, 0u, uint::min(len, col)) {
        if col < len {
            s1 = str::slice(s, col, len);
        } else { s1 = ~""; }
    } else { s1 = s; }
    log(debug, ~"pushing line: " + s1);
    lines.push(s1);
}

fn read_block_comment(rdr: string_reader, code_to_the_left: bool,
                                          comments: &mut ~[cmnt]) {
    debug!(">>> block comment");
    let p = rdr.last_pos;
    let mut lines: ~[~str] = ~[];
    let mut col: CharPos = rdr.col;
    bump(rdr);
    bump(rdr);

    // doc-comments are not really comments, they are attributes
    if rdr.curr == '*' || rdr.curr == '!' {
        while !(rdr.curr == '*' && nextch(rdr) == '/') && !is_eof(rdr) {
            bump(rdr);
        }
        if !is_eof(rdr) {
            bump(rdr);
            bump(rdr);
        }
        return;
    }

    let mut curr_line = ~"/*";
    let mut level: int = 1;
    while level > 0 {
        debug!("=== block comment level %d", level);
        if is_eof(rdr) {(rdr as reader).fatal(~"unterminated block comment");}
        if rdr.curr == '\n' {
            trim_whitespace_prefix_and_push_line(&mut lines, curr_line, col);
            curr_line = ~"";
            bump(rdr);
        } else {
            str::push_char(&mut curr_line, rdr.curr);
            if rdr.curr == '/' && nextch(rdr) == '*' {
                bump(rdr);
                bump(rdr);
                curr_line += ~"*";
                level += 1;
            } else {
                if rdr.curr == '*' && nextch(rdr) == '/' {
                    bump(rdr);
                    bump(rdr);
                    curr_line += ~"/";
                    level -= 1;
                } else { bump(rdr); }
            }
        }
    }
    if str::len(curr_line) != 0 {
        trim_whitespace_prefix_and_push_line(&mut lines, curr_line, col);
    }
    let mut style = if code_to_the_left { trailing } else { isolated };
    consume_non_eol_whitespace(rdr);
    if !is_eof(rdr) && rdr.curr != '\n' && vec::len(lines) == 1u {
        style = mixed;
    }
    debug!("<<< block comment");
    comments.push({style: style, lines: lines, pos: p});
}

fn peeking_at_comment(rdr: string_reader) -> bool {
    return ((rdr.curr == '/' && nextch(rdr) == '/') ||
         (rdr.curr == '/' && nextch(rdr) == '*')) ||
         (rdr.curr == '#' && nextch(rdr) == '!');
}

fn consume_comment(rdr: string_reader, code_to_the_left: bool,
                   comments: &mut ~[cmnt]) {
    debug!(">>> consume comment");
    if rdr.curr == '/' && nextch(rdr) == '/' {
        read_line_comments(rdr, code_to_the_left, comments);
    } else if rdr.curr == '/' && nextch(rdr) == '*' {
        read_block_comment(rdr, code_to_the_left, comments);
    } else if rdr.curr == '#' && nextch(rdr) == '!' {
        read_shebang_comment(rdr, code_to_the_left, comments);
    } else { fail; }
    debug!("<<< consume comment");
}

type lit = {lit: ~str, pos: BytePos};

fn gather_comments_and_literals(span_diagnostic: diagnostic::span_handler,
                                path: ~str,
                                srdr: io::Reader) ->
   {cmnts: ~[cmnt], lits: ~[lit]} {
    let src = @str::from_bytes(srdr.read_whole_stream());
    let itr = parse::token::mk_fake_ident_interner();
    let cm = CodeMap::new();
    let filemap = cm.new_filemap(path, src);
    let rdr = lexer::new_low_level_string_reader(
        span_diagnostic, filemap, itr);

    let mut comments: ~[cmnt] = ~[];
    let mut literals: ~[lit] = ~[];
    let mut first_read: bool = true;
    while !is_eof(rdr) {
        loop {
            let mut code_to_the_left = !first_read;
            consume_non_eol_whitespace(rdr);
            if rdr.curr == '\n' {
                code_to_the_left = false;
                consume_whitespace_counting_blank_lines(rdr, &mut comments);
            }
            while peeking_at_comment(rdr) {
                consume_comment(rdr, code_to_the_left, &mut comments);
                consume_whitespace_counting_blank_lines(rdr, &mut comments);
            }
            break;
        }


        let bstart = rdr.pos;
        rdr.next_token();
        //discard, and look ahead; we're working with internal state
        let {tok: tok, sp: sp} = rdr.peek();
        if token::is_lit(tok) {
            let s = get_str_from(rdr, bstart);
            literals.push({lit: s, pos: sp.lo});
            log(debug, ~"tok lit: " + s);
        } else {
            log(debug, ~"tok: " + token::to_str(rdr.interner, tok));
        }
        first_read = false;
    }
    return {cmnts: comments, lits: literals};
}
