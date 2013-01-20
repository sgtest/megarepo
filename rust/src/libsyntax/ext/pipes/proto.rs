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
use codemap::span;
use ext::base::ext_ctxt;
use ext::pipes::ast_builder::{path, append_types};

use core::cmp;
use core::dvec::DVec;
use core::to_str::ToStr;

enum direction { send, recv }

impl direction : cmp::Eq {
    pure fn eq(&self, other: &direction) -> bool {
        match ((*self), (*other)) {
            (send, send) => true,
            (recv, recv) => true,
            (send, _) => false,
            (recv, _) => false,
        }
    }
    pure fn ne(&self, other: &direction) -> bool { !(*self).eq(other) }
}

impl direction: ToStr {
    pure fn to_str() -> ~str {
        match self {
          send => ~"Send",
          recv => ~"Recv"
        }
    }
}

impl direction {
    fn reverse() -> direction {
        match self {
          send => recv,
          recv => send
        }
    }
}

struct next_state {
    state: ~str,
    tys: ~[@ast::Ty],
}

enum message {
    // name, span, data, current state, next state
    message(~str, span, ~[@ast::Ty], state, Option<next_state>)
}

impl message {
    fn name() -> ~str {
        match self {
          message(ref id, _, _, _, _) => (*id)
        }
    }

    fn span() -> span {
        match self {
          message(_, span, _, _, _) => span
        }
    }

    /// Return the type parameters actually used by this message
    fn get_params() -> ~[ast::ty_param] {
        match self {
          message(_, _, _, this, _) => this.ty_params
        }
    }
}

enum state {
    state_(@{
        id: uint,
        name: ~str,
        ident: ast::ident,
        span: span,
        dir: direction,
        ty_params: ~[ast::ty_param],
        messages: DVec<message>,
        proto: protocol,
    }),
}

impl state {
    fn add_message(name: ~str, span: span,
                   +data: ~[@ast::Ty], next: Option<next_state>) {
        self.messages.push(message(name, span, data, self,
                                   next));
    }

    fn filename() -> ~str {
        (*self).proto.filename()
    }

    fn data_name() -> ast::ident {
        self.ident
    }

    /// Returns the type that is used for the messages.
    fn to_ty(cx: ext_ctxt) -> @ast::Ty {
        cx.ty_path_ast_builder
            (path(~[cx.ident_of(self.name)],self.span).add_tys(
                cx.ty_vars(self.ty_params)))
    }

    /// Iterate over the states that can be reached in one message
    /// from this state.
    fn reachable(f: fn(state) -> bool) {
        for self.messages.each |m| {
            match *m {
              message(_, _, _, _, Some(next_state { state: ref id, _ })) => {
                let state = self.proto.get_state((*id));
                if !f(state) { break }
              }
              _ => ()
            }
        }
    }
}

type protocol = @protocol_;

fn protocol(name: ~str, +span: span) -> protocol {
    @protocol_(name, span)
}

fn protocol_(name: ~str, span: span) -> protocol_ {
    protocol_ {
        name: name,
        span: span,
        states: DVec(),
        bounded: None
    }
}

struct protocol_ {
    name: ~str,
    span: span,
    states: DVec<state>,

    mut bounded: Option<bool>,
}

impl protocol_ {

    /// Get a state.
    fn get_state(name: ~str) -> state {
        self.states.find(|i| i.name == name).get()
    }

    fn get_state_by_id(id: uint) -> state { self.states[id] }

    fn has_state(name: ~str) -> bool {
        self.states.find(|i| i.name == name).is_some()
    }

    fn filename() -> ~str {
        ~"proto://" + self.name
    }

    fn num_states() -> uint { self.states.len() }

    fn has_ty_params() -> bool {
        for self.states.each |s| {
            if s.ty_params.len() > 0 {
                return true;
            }
        }
        false
    }
    fn is_bounded() -> bool {
        let bounded = self.bounded.get();
        bounded
        //if bounded && self.has_ty_params() {
        //    debug!("protocol %s has is bounded, but type parameters\
        //            are not yet supported.",
        //           *self.name);
        //    false
        //}
        //else { bounded }
    }
}

impl protocol {
    fn add_state_poly(name: ~str, ident: ast::ident, dir: direction,
                      +ty_params: ~[ast::ty_param]) -> state {
        let messages = DVec();

        let state = state_(@{
            id: self.states.len(),
            name: name,
            ident: ident,
            span: self.span,
            dir: dir,
            ty_params: ty_params,
            messages: move messages,
            proto: self
        });

        self.states.push(state);
        state
    }
}

trait visitor<Tproto, Tstate, Tmessage> {
    fn visit_proto(proto: protocol, st: &[Tstate]) -> Tproto;
    fn visit_state(state: state, m: &[Tmessage]) -> Tstate;
    fn visit_message(name: ~str, spane: span, tys: &[@ast::Ty],
                     this: state, next: Option<next_state>) -> Tmessage;
}

fn visit<Tproto, Tstate, Tmessage, V: visitor<Tproto, Tstate, Tmessage>>(
    proto: protocol, visitor: V) -> Tproto {

    // the copy keywords prevent recursive use of dvec
    let states = do (copy proto.states).map_to_vec |&s| {
        let messages = do (copy s.messages).map_to_vec |&m| {
            let message(name, span, tys, this, next) = m;
            visitor.visit_message(name, span, tys, this, next)
        };
        visitor.visit_state(s, messages)
    };
    visitor.visit_proto(proto, states)
}
