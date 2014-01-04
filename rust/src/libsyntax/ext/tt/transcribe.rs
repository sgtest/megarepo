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
use codemap::{Span, DUMMY_SP};
use diagnostic::SpanHandler;
use ext::tt::macro_parser::{named_match, matched_seq, matched_nonterminal};
use parse::token::{EOF, INTERPOLATED, IDENT, Token, nt_ident};
use parse::token::{ident_to_str};
use parse::lexer::TokenAndSpan;

use std::cell::{Cell, RefCell};
use std::hashmap::HashMap;
use std::option;

///an unzipping of `token_tree`s
struct TtFrame {
    forest: @~[ast::token_tree],
    idx: Cell<uint>,
    dotdotdoted: bool,
    sep: Option<Token>,
    up: Option<@TtFrame>,
}

pub struct TtReader {
    sp_diag: @SpanHandler,
    // the unzipped tree:
    priv stack: RefCell<@TtFrame>,
    /* for MBE-style macro transcription */
    priv interpolations: RefCell<HashMap<Ident, @named_match>>,
    priv repeat_idx: RefCell<~[uint]>,
    priv repeat_len: RefCell<~[uint]>,
    /* cached: */
    cur_tok: RefCell<Token>,
    cur_span: RefCell<Span>,
}

/** This can do Macro-By-Example transcription. On the other hand, if
 *  `src` contains no `tt_seq`s and `tt_nonterminal`s, `interp` can (and
 *  should) be none. */
pub fn new_tt_reader(sp_diag: @SpanHandler,
                     interp: Option<HashMap<Ident,@named_match>>,
                     src: ~[ast::token_tree])
                     -> @TtReader {
    let r = @TtReader {
        sp_diag: sp_diag,
        stack: RefCell::new(@TtFrame {
            forest: @src,
            idx: Cell::new(0u),
            dotdotdoted: false,
            sep: None,
            up: option::None
        }),
        interpolations: match interp { /* just a convienience */
            None => RefCell::new(HashMap::new()),
            Some(x) => RefCell::new(x),
        },
        repeat_idx: RefCell::new(~[]),
        repeat_len: RefCell::new(~[]),
        /* dummy values, never read: */
        cur_tok: RefCell::new(EOF),
        cur_span: RefCell::new(DUMMY_SP),
    };
    tt_next_token(r); /* get cur_tok and cur_span set up */
    return r;
}

fn dup_tt_frame(f: @TtFrame) -> @TtFrame {
    @TtFrame {
        forest: @(*f.forest).clone(),
        idx: f.idx.clone(),
        dotdotdoted: f.dotdotdoted,
        sep: f.sep.clone(),
        up: match f.up {
            Some(up_frame) => Some(dup_tt_frame(up_frame)),
            None => None
        }
    }
}

pub fn dup_tt_reader(r: @TtReader) -> @TtReader {
    @TtReader {
        sp_diag: r.sp_diag,
        stack: RefCell::new(dup_tt_frame(r.stack.get())),
        repeat_idx: r.repeat_idx.clone(),
        repeat_len: r.repeat_len.clone(),
        cur_tok: r.cur_tok.clone(),
        cur_span: r.cur_span.clone(),
        interpolations: r.interpolations.clone(),
    }
}


fn lookup_cur_matched_by_matched(r: &TtReader, start: @named_match)
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
    let repeat_idx = r.repeat_idx.borrow();
    repeat_idx.get().iter().fold(start, red)
}

fn lookup_cur_matched(r: &TtReader, name: Ident) -> @named_match {
    let matched_opt = {
        let interpolations = r.interpolations.borrow();
        interpolations.get().find_copy(&name)
    };
    match matched_opt {
        Some(s) => lookup_cur_matched_by_matched(r, s),
        None => {
            r.sp_diag.span_fatal(r.cur_span.get(),
                                 format!("unknown macro variable `{}`",
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

fn lockstep_iter_size(t: &token_tree, r: &TtReader) -> lis {
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
                lis_contradiction(format!("Inconsistent lockstep iteration: \
                                           '{}' has {} items, but '{}' has {}",
                                           l_n, l_len, r_n, r_len))
            }
          }
        }
    }
    match *t {
      tt_delim(ref tts) | tt_seq(_, ref tts, _, _) => {
        tts.iter().fold(lis_unconstrained, |lis, tt| {
            let lis2 = lockstep_iter_size(tt, r);
            lis_merge(lis, lis2)
        })
      }
      tt_tok(..) => lis_unconstrained,
      tt_nonterminal(_, name) => match *lookup_cur_matched(r, name) {
        matched_nonterminal(_) => lis_unconstrained,
        matched_seq(ref ads, _) => lis_constraint(ads.len(), name)
      }
    }
}

// return the next token from the TtReader.
// EFFECT: advances the reader's token field
pub fn tt_next_token(r: &TtReader) -> TokenAndSpan {
    // XXX(pcwalton): Bad copy?
    let ret_val = TokenAndSpan {
        tok: r.cur_tok.get(),
        sp: r.cur_span.get(),
    };
    loop {
        {
            let mut stack = r.stack.borrow_mut();
            if stack.get().idx.get() < stack.get().forest.len() {
                break;
            }
        }

        /* done with this set; pop or repeat? */
        if !r.stack.get().dotdotdoted || {
                let repeat_idx = r.repeat_idx.borrow();
                let repeat_len = r.repeat_len.borrow();
                *repeat_idx.get().last() == *repeat_len.get().last() - 1
            } {

            match r.stack.get().up {
              None => {
                r.cur_tok.set(EOF);
                return ret_val;
              }
              Some(tt_f) => {
                if r.stack.get().dotdotdoted {
                    {
                        let mut repeat_idx = r.repeat_idx.borrow_mut();
                        let mut repeat_len = r.repeat_len.borrow_mut();
                        repeat_idx.get().pop();
                        repeat_len.get().pop();
                    }
                }

                r.stack.set(tt_f);
                r.stack.get().idx.set(r.stack.get().idx.get() + 1u);
              }
            }

        } else { /* repeat */
            r.stack.get().idx.set(0u);
            {
                let mut repeat_idx = r.repeat_idx.borrow_mut();
                repeat_idx.get()[repeat_idx.get().len() - 1u] += 1u;
            }
            match r.stack.get().sep.clone() {
              Some(tk) => {
                r.cur_tok.set(tk); /* repeat same span, I guess */
                return ret_val;
              }
              None => ()
            }
        }
    }
    loop { /* because it's easiest, this handles `tt_delim` not starting
    with a `tt_tok`, even though it won't happen */
        // XXX(pcwalton): Bad copy.
        match r.stack.get().forest[r.stack.get().idx.get()].clone() {
          tt_delim(tts) => {
            r.stack.set(@TtFrame {
                forest: tts,
                idx: Cell::new(0u),
                dotdotdoted: false,
                sep: None,
                up: option::Some(r.stack.get())
            });
            // if this could be 0-length, we'd need to potentially recur here
          }
          tt_tok(sp, tok) => {
            r.cur_span.set(sp);
            r.cur_tok.set(tok);
            r.stack.get().idx.set(r.stack.get().idx.get() + 1u);
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

                    r.stack.get().idx.set(r.stack.get().idx.get() + 1u);
                    return tt_next_token(r);
                } else {
                    {
                        let mut repeat_idx = r.repeat_idx.borrow_mut();
                        let mut repeat_len = r.repeat_len.borrow_mut();
                        repeat_len.get().push(len);
                        repeat_idx.get().push(0u);
                        r.stack.set(@TtFrame {
                            forest: tts,
                            idx: Cell::new(0u),
                            dotdotdoted: true,
                            sep: sep,
                            up: Some(r.stack.get())
                        });
                    }
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
                r.cur_span.set(sp);
                r.cur_tok.set(IDENT(sn,b));
                r.stack.get().idx.set(r.stack.get().idx.get() + 1u);
                return ret_val;
              }
              matched_nonterminal(ref other_whole_nt) => {
                // XXX(pcwalton): Bad copy.
                r.cur_span.set(sp);
                r.cur_tok.set(INTERPOLATED((*other_whole_nt).clone()));
                r.stack.get().idx.set(r.stack.get().idx.get() + 1u);
                return ret_val;
              }
              matched_seq(..) => {
                r.sp_diag.span_fatal(
                    r.cur_span.get(), /* blame the macro writer */
                    format!("variable '{}' is still repeating at this depth",
                         ident_to_str(&ident)));
              }
            }
          }
        }
    }

}
