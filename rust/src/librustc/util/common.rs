// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


use syntax::ast;
use syntax::codemap::{Span};
use syntax::visit;
use syntax::visit::Visitor;

use std::hashmap::HashSet;
use extra;

pub fn time<T>(do_it: bool, what: ~str, thunk: &fn() -> T) -> T {
    if !do_it { return thunk(); }
    let start = extra::time::precise_time_s();
    let rv = thunk();
    let end = extra::time::precise_time_s();
    printfln!("time: %3.3f s\t%s", end - start, what);
    rv
}

pub fn indent<R>(op: &fn() -> R) -> R {
    // Use in conjunction with the log post-processor like `src/etc/indenter`
    // to make debug output more readable.
    debug!(">>");
    let r = op();
    debug!("<< (Result = %?)", r);
    r
}

pub struct _indenter {
    _i: (),
}

impl Drop for _indenter {
    fn drop(&mut self) { debug!("<<"); }
}

pub fn _indenter(_i: ()) -> _indenter {
    _indenter {
        _i: ()
    }
}

pub fn indenter() -> _indenter {
    debug!(">>");
    _indenter(())
}

pub fn field_expr(f: ast::Field) -> @ast::Expr { return f.expr; }

pub fn field_exprs(fields: ~[ast::Field]) -> ~[@ast::Expr] {
    fields.map(|f| f.expr)
}

struct LoopQueryVisitor<'self> {
    p: &'self fn(&ast::Expr_) -> bool
}

impl<'self> Visitor<@mut bool> for LoopQueryVisitor<'self> {
    fn visit_expr(&mut self, e: @ast::Expr, flag: @mut bool) {
        *flag |= (self.p)(&e.node);
        match e.node {
          // Skip inner loops, since a break in the inner loop isn't a
          // break inside the outer loop
          ast::ExprLoop(*) | ast::ExprWhile(*) => {}
          _ => visit::walk_expr(self, e, flag)
        }
    }
}

// Takes a predicate p, returns true iff p is true for any subexpressions
// of b -- skipping any inner loops (loop, while, loop_body)
pub fn loop_query(b: &ast::Block, p: &fn(&ast::Expr_) -> bool) -> bool {
    let rs = @mut false;
    let mut v = LoopQueryVisitor {
        p: p,
    };
    visit::walk_block(&mut v, b, rs);
    return *rs;
}

struct BlockQueryVisitor<'self> {
    p: &'self fn(@ast::Expr) -> bool
}

impl<'self> Visitor<@mut bool> for BlockQueryVisitor<'self> {
    fn visit_expr(&mut self, e: @ast::Expr, flag: @mut bool) {
        *flag |= (self.p)(e);
        visit::walk_expr(self, e, flag)
    }
}

// Takes a predicate p, returns true iff p is true for any subexpressions
// of b -- skipping any inner loops (loop, while, loop_body)
pub fn block_query(b: &ast::Block, p: &fn(@ast::Expr) -> bool) -> bool {
    let rs = @mut false;
    let mut v = BlockQueryVisitor {
        p: p,
    };
    visit::walk_block(&mut v, b, rs);
    return *rs;
}

pub fn local_rhs_span(l: @ast::Local, def: Span) -> Span {
    match l.init {
      Some(i) => return i.span,
      _ => return def
    }
}

pub fn pluralize(n: uint, s: ~str) -> ~str {
    if n == 1 { s }
    else { fmt!("%ss", s) }
}

// A set of node IDs (used to keep track of which node IDs are for statements)
pub type stmt_set = @mut HashSet<ast::NodeId>;
