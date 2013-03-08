// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Earley-like parser for macros.
use ast;
use ast::{matcher, match_tok, match_seq, match_nonterminal, ident};
use codemap::{BytePos, mk_sp};
use codemap;
use parse::lexer::*; //resolve bug?
use parse::ParseSess;
use parse::parser::Parser;
use parse::token::{Token, EOF, to_str, nonterminal};
use parse::token;

use core::dvec::DVec;
use core::dvec;
use core::option::{Option, Some, None};
use core::str;
use core::uint;
use core::vec;
use std::oldmap::HashMap;

/* This is an Earley-like parser, without support for in-grammar nonterminals,
only by calling out to the main rust parser for named nonterminals (which it
commits to fully when it hits one in a grammar). This means that there are no
completer or predictor rules, and therefore no need to store one column per
token: instead, there's a set of current Earley items and a set of next
ones. Instead of NTs, we have a special case for Kleene star. The big-O, in
pathological cases, is worse than traditional Earley parsing, but it's an
easier fit for Macro-by-Example-style rules, and I think the overhead is
lower. (In order to prevent the pathological case, we'd need to lazily
construct the resulting `named_match`es at the very end. It'd be a pain,
and require more memory to keep around old items, but it would also save
overhead)*/

/* Quick intro to how the parser works:

A 'position' is a dot in the middle of a matcher, usually represented as a
dot. For example `· a $( a )* a b` is a position, as is `a $( · a )* a b`.

The parser walks through the input a character at a time, maintaining a list
of items consistent with the current position in the input string: `cur_eis`.

As it processes them, it fills up `eof_eis` with items that would be valid if
the macro invocation is now over, `bb_eis` with items that are waiting on
a Rust nonterminal like `$e:expr`, and `next_eis` with items that are waiting
on the a particular token. Most of the logic concerns moving the · through the
repetitions indicated by Kleene stars. It only advances or calls out to the
real Rust parser when no `cur_eis` items remain

Example: Start parsing `a a a a b` against [· a $( a )* a b].

Remaining input: `a a a a b`
next_eis: [· a $( a )* a b]

- - - Advance over an `a`. - - -

Remaining input: `a a a b`
cur: [a · $( a )* a b]
Descend/Skip (first item).
next: [a $( · a )* a b]  [a $( a )* · a b].

- - - Advance over an `a`. - - -

Remaining input: `a a b`
cur: [a $( a · )* a b]  next: [a $( a )* a · b]
Finish/Repeat (first item)
next: [a $( a )* · a b]  [a $( · a )* a b]  [a $( a )* a · b]

- - - Advance over an `a`. - - - (this looks exactly like the last step)

Remaining input: `a b`
cur: [a $( a · )* a b]  next: [a $( a )* a · b]
Finish/Repeat (first item)
next: [a $( a )* · a b]  [a $( · a )* a b]  [a $( a )* a · b]

- - - Advance over an `a`. - - - (this looks exactly like the last step)

Remaining input: `b`
cur: [a $( a · )* a b]  next: [a $( a )* a · b]
Finish/Repeat (first item)
next: [a $( a )* · a b]  [a $( · a )* a b]

- - - Advance over a `b`. - - -

Remaining input: ``
eof: [a $( a )* a b ·]

 */


/* to avoid costly uniqueness checks, we require that `match_seq` always has a
nonempty body. */

pub enum matcher_pos_up { /* to break a circularity */
    matcher_pos_up(Option<~MatcherPos>)
}

pub fn is_some(&&mpu: matcher_pos_up) -> bool {
    match &mpu {
      &matcher_pos_up(None) => false,
      _ => true
    }
}

pub struct MatcherPos {
    elts: ~[ast::matcher], // maybe should be /&? Need to understand regions.
    sep: Option<Token>,
    idx: uint,
    up: matcher_pos_up, // mutable for swapping only
    matches: ~[DVec<@named_match>],
    match_lo: uint, match_hi: uint,
    sp_lo: BytePos,
}

pub fn copy_up(&& mpu: matcher_pos_up) -> ~MatcherPos {
    match &mpu {
      &matcher_pos_up(Some(ref mp)) => copy (*mp),
      _ => fail!()
    }
}

pub fn count_names(ms: &[matcher]) -> uint {
    vec::foldl(0u, ms, |ct, m| {
        ct + match m.node {
          match_tok(_) => 0u,
          match_seq(ref more_ms, _, _, _, _) => count_names((*more_ms)),
          match_nonterminal(_,_,_) => 1u
        }})
}

#[allow(non_implicitly_copyable_typarams)]
pub fn initial_matcher_pos(+ms: ~[matcher], +sep: Option<Token>, lo: BytePos)
                        -> ~MatcherPos {
    let mut match_idx_hi = 0u;
    for ms.each |elt| {
        match elt.node {
          match_tok(_) => (),
          match_seq(_,_,_,_,hi) => {
            match_idx_hi = hi;       // it is monotonic...
          }
          match_nonterminal(_,_,pos) => {
            match_idx_hi = pos+1u;  // ...so latest is highest
          }
        }
    }
    let matches = vec::from_fn(count_names(ms), |_i| dvec::DVec());
    ~MatcherPos {
        elts: ms,
        sep: sep,
        idx: 0u,
        up: matcher_pos_up(None),
        matches: matches,
        match_lo: 0u,
        match_hi: match_idx_hi,
        sp_lo: lo
    }
}

// named_match is a pattern-match result for a single ast::match_nonterminal:
// so it is associated with a single ident in a parse, and all
// matched_nonterminals in the named_match have the same nonterminal type
// (expr, item, etc). All the leaves in a single named_match correspond to a
// single matcher_nonterminal in the ast::matcher that produced it.
//
// It should probably be renamed, it has more or less exact correspondence to
// ast::match nodes, and the in-memory structure of a particular named_match
// represents the match that occurred when a particular subset of an
// ast::match -- those ast::matcher nodes leading to a single
// match_nonterminal -- was applied to a particular token tree.
//
// The width of each matched_seq in the named_match, and the identity of the
// matched_nonterminals, will depend on the token tree it was applied to: each
// matched_seq corresponds to a single match_seq in the originating
// ast::matcher. The depth of the named_match structure will therefore depend
// only on the nesting depth of ast::match_seqs in the originating
// ast::matcher it was derived from.

pub enum named_match {
    matched_seq(~[@named_match], codemap::span),
    matched_nonterminal(nonterminal)
}

pub type earley_item = ~MatcherPos;

pub fn nameize(p_s: @mut ParseSess, ms: ~[matcher], res: ~[@named_match])
            -> HashMap<ident,@named_match> {
    fn n_rec(p_s: @mut ParseSess, m: matcher, res: ~[@named_match],
             ret_val: HashMap<ident, @named_match>) {
        match m {
          codemap::spanned {node: match_tok(_), _} => (),
          codemap::spanned {node: match_seq(ref more_ms, _, _, _, _), _} => {
            for (*more_ms).each() |next_m| {
                n_rec(p_s, *next_m, res, ret_val)
            };
          }
          codemap::spanned {
                node: match_nonterminal(bind_name, _, idx), span: sp
          } => {
            if ret_val.contains_key(&bind_name) {
                p_s.span_diagnostic.span_fatal(sp, ~"Duplicated bind name: "+
                                               *p_s.interner.get(bind_name))
            }
            ret_val.insert(bind_name, res[idx]);
          }
        }
    }
    let ret_val = HashMap();
    for ms.each() |m| { n_rec(p_s, *m, res, ret_val) }
    return ret_val;
}

pub enum parse_result {
    success(HashMap<ident, @named_match>),
    failure(codemap::span, ~str),
    error(codemap::span, ~str)
}

pub fn parse_or_else(
    sess: @mut ParseSess,
    +cfg: ast::crate_cfg,
    rdr: reader,
    ms: ~[matcher]
) -> HashMap<ident, @named_match> {
    match parse(sess, cfg, rdr, ms) {
      success(m) => m,
      failure(sp, ref str) => sess.span_diagnostic.span_fatal(sp, (*str)),
      error(sp, ref str) => sess.span_diagnostic.span_fatal(sp, (*str))
    }
}

pub fn parse(
    sess: @mut ParseSess,
    cfg: ast::crate_cfg,
    rdr: reader,
    ms: ~[matcher]
) -> parse_result {
    let mut cur_eis = ~[];
    cur_eis.push(initial_matcher_pos(copy ms, None, rdr.peek().sp.lo));

    loop {
        let mut bb_eis = ~[]; // black-box parsed by parser.rs
        let mut next_eis = ~[]; // or proceed normally
        let mut eof_eis = ~[];

        let TokenAndSpan {tok: tok, sp: sp} = rdr.peek();

        /* we append new items to this while we go */
        while cur_eis.len() > 0u { /* for each Earley Item */
            let mut ei = cur_eis.pop();

            let idx = ei.idx;
            let len = ei.elts.len();

            /* at end of sequence */
            if idx >= len {
                // can't move out of `match`es, so:
                if is_some(ei.up) {
                    // hack: a matcher sequence is repeating iff it has a
                    // parent (the top level is just a container)


                    // disregard separator, try to go up
                    // (remove this condition to make trailing seps ok)
                    if idx == len {
                        // pop from the matcher position

                        let mut new_pos = copy_up(ei.up);

                        // update matches (the MBE "parse tree") by appending
                        // each tree as a subtree.

                        // I bet this is a perf problem: we're preemptively
                        // doing a lot of array work that will get thrown away
                        // most of the time.

                        // Only touch the binders we have actually bound
                        for uint::range(ei.match_lo, ei.match_hi) |idx| {
                            let sub = ei.matches[idx].get();
                            new_pos.matches[idx]
                                .push(@matched_seq(sub,
                                                   mk_sp(ei.sp_lo,
                                                         sp.hi)));
                        }

                        new_pos.idx += 1;
                        cur_eis.push(new_pos);
                    }

                    // can we go around again?

                    // the *_t vars are workarounds for the lack of unary move
                    match copy ei.sep {
                      Some(ref t) if idx == len => { // we need a separator
                        if tok == (*t) { //pass the separator
                            let mut ei_t = ei;
                            ei_t.idx += 1;
                            next_eis.push(ei_t);
                        }
                      }
                      _ => { // we don't need a separator
                        let mut ei_t = ei;
                        ei_t.idx = 0;
                        cur_eis.push(ei_t);
                      }
                    }
                } else {
                    eof_eis.push(ei);
                }
            } else {
                match copy ei.elts[idx].node {
                  /* need to descend into sequence */
                  match_seq(ref matchers, ref sep, zero_ok,
                            match_idx_lo, match_idx_hi) => {
                    if zero_ok {
                        let mut new_ei = copy ei;
                        new_ei.idx += 1u;
                        //we specifically matched zero repeats.
                        for uint::range(match_idx_lo, match_idx_hi) |idx| {
                            new_ei.matches[idx].push(@matched_seq(~[], sp));
                        }

                        cur_eis.push(new_ei);
                    }

                    let matches = vec::map(ei.matches, // fresh, same size:
                                           |_m| DVec::<@named_match>());
                    let ei_t = ei;
                    cur_eis.push(~MatcherPos {
                        elts: copy *matchers,
                        sep: copy *sep,
                        idx: 0u,
                        up: matcher_pos_up(Some(ei_t)),
                        matches: matches,
                        match_lo: match_idx_lo, match_hi: match_idx_hi,
                        sp_lo: sp.lo
                    });
                  }
                  match_nonterminal(_,_,_) => { bb_eis.push(ei) }
                  match_tok(ref t) => {
                    let mut ei_t = ei;
                    if (*t) == tok {
                        ei_t.idx += 1;
                        next_eis.push(ei_t);
                    }
                  }
                }
            }
        }

        /* error messages here could be improved with links to orig. rules */
        if tok == EOF {
            if eof_eis.len() == 1u {
                return success(
                    nameize(sess, ms,
                            eof_eis[0u].matches.map(|dv| dv.pop())));
            } else if eof_eis.len() > 1u {
                return error(sp, ~"Ambiguity: multiple successful parses");
            } else {
                return failure(sp, ~"Unexpected end of macro invocation");
            }
        } else {
            if (bb_eis.len() > 0u && next_eis.len() > 0u)
                || bb_eis.len() > 1u {
                let nts = str::connect(vec::map(bb_eis, |ei| {
                    match ei.elts[ei.idx].node {
                      match_nonterminal(bind,name,_) => {
                        fmt!("%s ('%s')", *sess.interner.get(name),
                             *sess.interner.get(bind))
                      }
                      _ => fail!()
                    } }), ~" or ");
                return error(sp, fmt!(
                    "Local ambiguity: multiple parsing options: \
                     built-in NTs %s or %u other options.",
                    nts, next_eis.len()));
            } else if (bb_eis.len() == 0u && next_eis.len() == 0u) {
                return failure(sp, ~"No rules expected the token: "
                            + to_str(rdr.interner(), &tok));
            } else if (next_eis.len() > 0u) {
                /* Now process the next token */
                while(next_eis.len() > 0u) {
                    cur_eis.push(next_eis.pop());
                }
                rdr.next_token();
            } else /* bb_eis.len() == 1 */ {
                let rust_parser = Parser(sess, copy cfg, rdr.dup());

                let mut ei = bb_eis.pop();
                match ei.elts[ei.idx].node {
                  match_nonterminal(_, name, idx) => {
                    ei.matches[idx].push(@matched_nonterminal(
                        parse_nt(rust_parser, *sess.interner.get(name))));
                    ei.idx += 1u;
                  }
                  _ => fail!()
                }
                cur_eis.push(ei);

                for rust_parser.tokens_consumed.times() || {
                    rdr.next_token();
                }
            }
        }

        fail_unless!(cur_eis.len() > 0u);
    }
}

pub fn parse_nt(p: Parser, name: ~str) -> nonterminal {
    match name {
      ~"item" => match p.parse_item(~[]) {
        Some(i) => token::nt_item(i),
        None => p.fatal(~"expected an item keyword")
      },
      ~"block" => token::nt_block(p.parse_block()),
      ~"stmt" => token::nt_stmt(p.parse_stmt(~[])),
      ~"pat" => token::nt_pat(p.parse_pat(true)),
      ~"expr" => token::nt_expr(p.parse_expr()),
      ~"ty" => token::nt_ty(p.parse_ty(false /* no need to disambiguate*/)),
      // this could be handled like a token, since it is one
      ~"ident" => match *p.token {
        token::IDENT(sn,b) => { p.bump(); token::nt_ident(sn,b) }
        _ => p.fatal(~"expected ident, found "
                     + token::to_str(p.reader.interner(), &copy *p.token))
      },
      ~"path" => token::nt_path(p.parse_path_with_tps(false)),
      ~"tt" => {
        *p.quote_depth += 1u; //but in theory, non-quoted tts might be useful
        let res = token::nt_tt(@p.parse_token_tree());
        *p.quote_depth -= 1u;
        res
      }
      ~"matchers" => token::nt_matchers(p.parse_matchers()),
      _ => p.fatal(~"Unsupported builtin nonterminal parser: " + name)
    }
}

// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
