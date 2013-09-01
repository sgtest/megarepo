// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use ast;
use ast::{token_tree, tt_delim, tt_tok, tt_seq, tt_nonterminal,Ident};
use codemap::{Span, dummy_sp};
use diagnostic::span_handler;
use ext::tt::macro_parser::{named_match, matched_seq, matched_nonterminal};
use parse::token::{EOF, INTERPOLATED, IDENT, Token, nt_ident};
use parse::token::{ident_to_str};
use parse::lexer::TokenAndSpan;

use std::hashmap::HashMap;
use std::option;

///an unzipping of `token_tree`s
struct TtFrame {
    forest: @mut ~[ast::token_tree],
    idx: uint,
    dotdotdoted: bool,
    sep: Option<Token>,
    up: Option<@mut TtFrame>,
}

pub struct TtReader {
    sp_diag: @mut span_handler,
    // the unzipped tree:
    stack: @mut TtFrame,
    /* for MBE-style macro transcription */
    interpolations: HashMap<Ident, @named_match>,
    repeat_idx: ~[uint],
    repeat_len: ~[uint],
    /* cached: */
    cur_tok: Token,
    cur_span: Span
}

/** This can do Macro-By-Example transcription. On the other hand, if
 *  `src` contains no `tt_seq`s and `tt_nonterminal`s, `interp` can (and
 *  should) be none. */
pub fn new_tt_reader(sp_diag: @mut span_handler,
                     interp: Option<HashMap<Ident,@named_match>>,
                     src: ~[ast::token_tree])
                  -> @mut TtReader {
    let r = @mut TtReader {
        sp_diag: sp_diag,
        stack: @mut TtFrame {
            forest: @mut src,
            idx: 0u,
            dotdotdoted: false,
            sep: None,
            up: option::None
        },
        interpolations: match interp { /* just a convienience */
            None => HashMap::new(),
            Some(x) => x
        },
        repeat_idx: ~[],
        repeat_len: ~[],
        /* dummy values, never read: */
        cur_tok: EOF,
        cur_span: dummy_sp()
    };
    tt_next_token(r); /* get cur_tok and cur_span set up */
    return r;
}

fn dup_tt_frame(f: @mut TtFrame) -> @mut TtFrame {
    @mut TtFrame {
        forest: @mut (*f.forest).clone(),
        idx: f.idx,
        dotdotdoted: f.dotdotdoted,
        sep: f.sep.clone(),
        up: match f.up {
            Some(up_frame) => Some(dup_tt_frame(up_frame)),
            None => None
        }
    }
}

pub fn dup_tt_reader(r: @mut TtReader) -> @mut TtReader {
    @mut TtReader {
        sp_diag: r.sp_diag,
        stack: dup_tt_frame(r.stack),
        repeat_idx: r.repeat_idx.clone(),
        repeat_len: r.repeat_len.clone(),
        cur_tok: r.cur_tok.clone(),
        cur_span: r.cur_span,
        interpolations: r.interpolations.clone(),
    }
}


fn lookup_cur_matched_by_matched(r: &mut TtReader,
                                      start: @named_match)
                                   -> @named_match {
    fn red(ad: @named_match, idx: &uint) -> @named_match {
        match *ad {
          matched_nonterminal(_) => {
            // end of the line; duplicate henceforth
            ad
          }
          matched_seq(ref ads, _) => ads[*idx]
        }
    }
    r.repeat_idx.iter().fold(start, red)
}

fn lookup_cur_matched(r: &mut TtReader, name: Ident) -> @named_match {
    match r.interpolations.find_copy(&name) {
        Some(s) => lookup_cur_matched_by_matched(r, s),
        None => {
            r.sp_diag.span_fatal(r.cur_span, fmt!("unknown macro variable `%s`",
                                                  ident_to_str(&name)));
        }
    }
}

#[deriving(Clone)]
enum lis {
    lis_unconstrained,
    lis_constraint(uint, Ident),
    lis_contradiction(~str),
}

fn lockstep_iter_size(t: &token_tree, r: &mut TtReader) -> lis {
    fn lis_merge(lhs: lis, rhs: lis) -> lis {
        match lhs {
          lis_unconstrained => rhs.clone(),
          lis_contradiction(_) => lhs.clone(),
          lis_constraint(l_len, ref l_id) => match rhs {
            lis_unconstrained => lhs.clone(),
            lis_contradiction(_) => rhs.clone(),
            lis_constraint(r_len, _) if l_len == r_len => lhs.clone(),
            lis_constraint(r_len, ref r_id) => {
                let l_n = ident_to_str(l_id);
                let r_n = ident_to_str(r_id);
                lis_contradiction(fmt!("Inconsistent lockstep iteration: \
                                       '%s' has %u items, but '%s' has %u",
                                        l_n, l_len, r_n, r_len))
            }
          }
        }
    }
    match *t {
      tt_delim(ref tts) | tt_seq(_, ref tts, _, _) => {
        do tts.iter().fold(lis_unconstrained) |lis, tt| {
            let lis2 = lockstep_iter_size(tt, r);
            lis_merge(lis, lis2)
        }
      }
      tt_tok(*) => lis_unconstrained,
      tt_nonterminal(_, name) => match *lookup_cur_matched(r, name) {
        matched_nonterminal(_) => lis_unconstrained,
        matched_seq(ref ads, _) => lis_constraint(ads.len(), name)
      }
    }
}

// return the next token from the TtReader.
// EFFECT: advances the reader's token field
pub fn tt_next_token(r: &mut TtReader) -> TokenAndSpan {
    // XXX(pcwalton): Bad copy?
    let ret_val = TokenAndSpan {
        tok: r.cur_tok.clone(),
        sp: r.cur_span,
    };
    loop {
        {
            let stack = &mut *r.stack;
            let forest = &mut *stack.forest;
            if stack.idx < forest.len() {
                break;
            }
        }

        /* done with this set; pop or repeat? */
        if ! r.stack.dotdotdoted
            || { *r.repeat_idx.last() == *r.repeat_len.last() - 1 } {

            match r.stack.up {
              None => {
                r.cur_tok = EOF;
                return ret_val;
              }
              Some(tt_f) => {
                if r.stack.dotdotdoted {
                    r.repeat_idx.pop();
                    r.repeat_len.pop();
                }

                r.stack = tt_f;
                r.stack.idx += 1u;
              }
            }

        } else { /* repeat */
            r.stack.idx = 0u;
            r.repeat_idx[r.repeat_idx.len() - 1u] += 1u;
            match r.stack.sep.clone() {
              Some(tk) => {
                r.cur_tok = tk; /* repeat same span, I guess */
                return ret_val;
              }
              None => ()
            }
        }
    }
    loop { /* because it's easiest, this handles `tt_delim` not starting
    with a `tt_tok`, even though it won't happen */
        // XXX(pcwalton): Bad copy.
        match r.stack.forest[r.stack.idx].clone() {
          tt_delim(tts) => {
            r.stack = @mut TtFrame {
                forest: tts,
                idx: 0u,
                dotdotdoted: false,
                sep: None,
                up: option::Some(r.stack)
            };
            // if this could be 0-length, we'd need to potentially recur here
          }
          tt_tok(sp, tok) => {
            r.cur_span = sp;
            r.cur_tok = tok;
            r.stack.idx += 1u;
            return ret_val;
          }
          tt_seq(sp, tts, sep, zerok) => {
            // XXX(pcwalton): Bad copy.
            let t = tt_seq(sp, tts, sep.clone(), zerok);
            match lockstep_iter_size(&t, r) {
              lis_unconstrained => {
                r.sp_diag.span_fatal(
                    sp, /* blame macro writer */
                      "attempted to repeat an expression \
                       containing no syntax \
                       variables matched as repeating at this depth");
                  }
                  lis_contradiction(ref msg) => {
                      /* FIXME #2887 blame macro invoker instead*/
                      r.sp_diag.span_fatal(sp, (*msg));
                  }
                  lis_constraint(len, _) => {
                    if len == 0 {
                      if !zerok {
                        r.sp_diag.span_fatal(sp, /* FIXME #2887 blame invoker
                        */
                                             "this must repeat at least \
                                              once");
                          }

                    r.stack.idx += 1u;
                    return tt_next_token(r);
                } else {
                    r.repeat_len.push(len);
                    r.repeat_idx.push(0u);
                    r.stack = @mut TtFrame {
                        forest: tts,
                        idx: 0u,
                        dotdotdoted: true,
                        sep: sep,
                        up: Some(r.stack)
                    };
                }
              }
            }
          }
          // FIXME #2887: think about span stuff here
          tt_nonterminal(sp, ident) => {
            match *lookup_cur_matched(r, ident) {
              /* sidestep the interpolation tricks for ident because
              (a) idents can be in lots of places, so it'd be a pain
              (b) we actually can, since it's a token. */
              matched_nonterminal(nt_ident(~sn,b)) => {
                r.cur_span = sp; r.cur_tok = IDENT(sn,b);
                r.stack.idx += 1u;
                return ret_val;
              }
              matched_nonterminal(ref other_whole_nt) => {
                // XXX(pcwalton): Bad copy.
                r.cur_span = sp;
                r.cur_tok = INTERPOLATED((*other_whole_nt).clone());
                r.stack.idx += 1u;
                return ret_val;
              }
              matched_seq(*) => {
                r.sp_diag.span_fatal(
                    r.cur_span, /* blame the macro writer */
                    fmt!("variable '%s' is still repeating at this depth",
                         ident_to_str(&ident)));
              }
            }
          }
        }
    }

}
