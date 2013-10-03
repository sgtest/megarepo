// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A 'lint' check is a kind of miscellaneous constraint that a user _might_
//! want to enforce, but might reasonably want to permit as well, on a
//! module-by-module basis. They contrast with static constraints enforced by
//! other phases of the compiler, which are generally required to hold in order
//! to compile the program at all.
//!
//! The lint checking is all consolidated into one pass which runs just before
//! translation to LLVM bytecode. Throughout compilation, lint warnings can be
//! added via the `add_lint` method on the Session structure. This requires a
//! span and an id of the node that the lint is being added to. The lint isn't
//! actually emitted at that time because it is unknown what the actual lint
//! level at that location is.
//!
//! To actually emit lint warnings/errors, a separate pass is used just before
//! translation. A context keeps track of the current state of all lint levels.
//! Upon entering a node of the ast which can modify the lint settings, the
//! previous lint state is pushed onto a stack and the ast is then recursed
//! upon.  As the ast is traversed, this keeps track of the current lint level
//! for all lint attributes.
//!
//! To add a new lint warning, all you need to do is to either invoke `add_lint`
//! on the session at the appropriate time, or write a few linting functions and
//! modify the Context visitor appropriately. If you're adding lints from the
//! Context itself, span_lint should be used instead of add_lint.

use driver::session;
use middle::ty;
use middle::pat_util;
use metadata::csearch;
use util::ppaux::{ty_to_str};

use std::cmp;
use std::hashmap::HashMap;
use std::i16;
use std::i32;
use std::i64;
use std::i8;
use std::u16;
use std::u32;
use std::u64;
use std::u8;
use extra::smallintmap::SmallIntMap;
use syntax::ast_map;
use syntax::attr;
use syntax::attr::{AttrMetaMethods, AttributeMethods};
use syntax::codemap::Span;
use syntax::codemap;
use syntax::parse::token;
use syntax::{ast, ast_util, visit};
use syntax::visit::Visitor;

#[deriving(Clone, Eq)]
pub enum lint {
    ctypes,
    cstack,
    unused_imports,
    unnecessary_qualification,
    while_true,
    path_statement,
    unrecognized_lint,
    non_camel_case_types,
    non_uppercase_statics,
    non_uppercase_pattern_statics,
    type_limits,
    unused_unsafe,

    managed_heap_memory,
    owned_heap_memory,
    heap_memory,

    unused_variable,
    dead_assignment,
    unused_mut,
    unnecessary_allocation,

    missing_doc,
    unreachable_code,

    deprecated,
    experimental,
    unstable,

    warnings,
}

pub fn level_to_str(lv: level) -> &'static str {
    match lv {
      allow => "allow",
      warn => "warn",
      deny => "deny",
      forbid => "forbid"
    }
}

#[deriving(Clone, Eq, Ord)]
pub enum level {
    allow, warn, deny, forbid
}

#[deriving(Clone, Eq)]
pub struct LintSpec {
    lint: lint,
    desc: &'static str,
    default: level
}

impl Ord for LintSpec {
    fn lt(&self, other: &LintSpec) -> bool { self.default < other.default }
}

pub type LintDict = HashMap<&'static str, LintSpec>;

#[deriving(Eq)]
enum LintSource {
    Node(Span),
    Default,
    CommandLine
}

static lint_table: &'static [(&'static str, LintSpec)] = &[
    ("ctypes",
     LintSpec {
        lint: ctypes,
        desc: "proper use of std::libc types in foreign modules",
        default: warn
     }),

    ("cstack",
     LintSpec {
        lint: cstack,
        desc: "only invoke foreign functions from fixedstacksegment fns",
        default: deny
     }),

    ("unused_imports",
     LintSpec {
        lint: unused_imports,
        desc: "imports that are never used",
        default: warn
     }),

    ("unnecessary_qualification",
     LintSpec {
        lint: unnecessary_qualification,
        desc: "detects unnecessarily qualified names",
        default: allow
     }),

    ("while_true",
     LintSpec {
        lint: while_true,
        desc: "suggest using loop { } instead of while(true) { }",
        default: warn
     }),

    ("path_statement",
     LintSpec {
        lint: path_statement,
        desc: "path statements with no effect",
        default: warn
     }),

    ("unrecognized_lint",
     LintSpec {
        lint: unrecognized_lint,
        desc: "unrecognized lint attribute",
        default: warn
     }),

    ("non_camel_case_types",
     LintSpec {
        lint: non_camel_case_types,
        desc: "types, variants and traits should have camel case names",
        default: allow
     }),

    ("non_uppercase_statics",
     LintSpec {
         lint: non_uppercase_statics,
         desc: "static constants should have uppercase identifiers",
         default: allow
     }),

    ("non_uppercase_pattern_statics",
     LintSpec {
         lint: non_uppercase_pattern_statics,
         desc: "static constants in match patterns should be all caps",
         default: warn
     }),

    ("managed_heap_memory",
     LintSpec {
        lint: managed_heap_memory,
        desc: "use of managed (@ type) heap memory",
        default: allow
     }),

    ("owned_heap_memory",
     LintSpec {
        lint: owned_heap_memory,
        desc: "use of owned (~ type) heap memory",
        default: allow
     }),

    ("heap_memory",
     LintSpec {
        lint: heap_memory,
        desc: "use of any (~ type or @ type) heap memory",
        default: allow
     }),

    ("type_limits",
     LintSpec {
        lint: type_limits,
        desc: "comparisons made useless by limits of the types involved",
        default: warn
     }),

    ("unused_unsafe",
     LintSpec {
        lint: unused_unsafe,
        desc: "unnecessary use of an `unsafe` block",
        default: warn
    }),

    ("unused_variable",
     LintSpec {
        lint: unused_variable,
        desc: "detect variables which are not used in any way",
        default: warn
    }),

    ("dead_assignment",
     LintSpec {
        lint: dead_assignment,
        desc: "detect assignments that will never be read",
        default: warn
    }),

    ("unused_mut",
     LintSpec {
        lint: unused_mut,
        desc: "detect mut variables which don't need to be mutable",
        default: warn
    }),

    ("unnecessary_allocation",
     LintSpec {
        lint: unnecessary_allocation,
        desc: "detects unnecessary allocations that can be eliminated",
        default: warn
    }),

    ("missing_doc",
     LintSpec {
        lint: missing_doc,
        desc: "detects missing documentation for public members",
        default: allow
    }),

    ("unreachable_code",
     LintSpec {
        lint: unreachable_code,
        desc: "detects unreachable code",
        default: warn
    }),

    ("deprecated",
     LintSpec {
        lint: deprecated,
        desc: "detects use of #[deprecated] items",
        default: warn
    }),

    ("experimental",
     LintSpec {
        lint: experimental,
        desc: "detects use of #[experimental] items",
        default: warn
    }),

    ("unstable",
     LintSpec {
        lint: unstable,
        desc: "detects use of #[unstable] items (incl. items with no stability attribute)",
        default: allow
    }),

    ("warnings",
     LintSpec {
        lint: warnings,
        desc: "mass-change the level for lints which produce warnings",
        default: warn
    }),
];

/*
  Pass names should not contain a '-', as the compiler normalizes
  '-' to '_' in command-line flags
 */
pub fn get_lint_dict() -> LintDict {
    let mut map = HashMap::new();
    for &(k, v) in lint_table.iter() {
        map.insert(k, v);
    }
    return map;
}

struct Context {
    // All known lint modes (string versions)
    dict: @LintDict,
    // Current levels of each lint warning
    cur: SmallIntMap<(level, LintSource)>,
    // context we're checking in (used to access fields like sess)
    tcx: ty::ctxt,

    // When recursing into an attributed node of the ast which modifies lint
    // levels, this stack keeps track of the previous lint levels of whatever
    // was modified.
    lint_stack: ~[(lint, level, LintSource)],
}

impl Context {
    fn get_level(&self, lint: lint) -> level {
        match self.cur.find(&(lint as uint)) {
          Some(&(lvl, _)) => lvl,
          None => allow
        }
    }

    fn get_source(&self, lint: lint) -> LintSource {
        match self.cur.find(&(lint as uint)) {
          Some(&(_, src)) => src,
          None => Default
        }
    }

    fn set_level(&mut self, lint: lint, level: level, src: LintSource) {
        if level == allow {
            self.cur.remove(&(lint as uint));
        } else {
            self.cur.insert(lint as uint, (level, src));
        }
    }

    fn lint_to_str(&self, lint: lint) -> &'static str {
        for (k, v) in self.dict.iter() {
            if v.lint == lint {
                return *k;
            }
        }
        fail2!("unregistered lint {:?}", lint);
    }

    fn span_lint(&self, lint: lint, span: Span, msg: &str) {
        let (level, src) = match self.cur.find(&(lint as uint)) {
            None => { return }
            Some(&(warn, src)) => (self.get_level(warnings), src),
            Some(&pair) => pair,
        };
        if level == allow { return }

        let mut note = None;
        let msg = match src {
            Default | CommandLine => {
                format!("{} [-{} {}{}]", msg, match level {
                        warn => 'W', deny => 'D', forbid => 'F',
                        allow => fail2!()
                    }, self.lint_to_str(lint).replace("_", "-"),
                    if src == Default { " (default)" } else { "" })
            },
            Node(src) => {
                note = Some(src);
                msg.to_str()
            }
        };
        match level {
            warn =>          { self.tcx.sess.span_warn(span, msg); }
            deny | forbid => { self.tcx.sess.span_err(span, msg);  }
            allow => fail2!(),
        }

        for &span in note.iter() {
            self.tcx.sess.span_note(span, "lint level defined here");
        }
    }

    /**
     * Merge the lints specified by any lint attributes into the
     * current lint context, call the provided function, then reset the
     * lints in effect to their previous state.
     */
    fn with_lint_attrs(&mut self, attrs: &[ast::Attribute],
                       f: &fn(&mut Context)) {
        // Parse all of the lint attributes, and then add them all to the
        // current dictionary of lint information. Along the way, keep a history
        // of what we changed so we can roll everything back after invoking the
        // specified closure
        let mut pushed = 0u;
        do each_lint(self.tcx.sess, attrs) |meta, level, lintname| {
            match self.dict.find_equiv(&lintname) {
                None => {
                    self.span_lint(
                        unrecognized_lint,
                        meta.span,
                        format!("unknown `{}` attribute: `{}`",
                        level_to_str(level), lintname));
                }
                Some(lint) => {
                    let lint = lint.lint;
                    let now = self.get_level(lint);
                    if now == forbid && level != forbid {
                        self.tcx.sess.span_err(meta.span,
                        format!("{}({}) overruled by outer forbid({})",
                        level_to_str(level),
                        lintname, lintname));
                    } else if now != level {
                        let src = self.get_source(lint);
                        self.lint_stack.push((lint, now, src));
                        pushed += 1;
                        self.set_level(lint, level, Node(meta.span));
                    }
                }
            }
            true
        };

        f(self);

        // rollback
        do pushed.times {
            let (lint, lvl, src) = self.lint_stack.pop();
            self.set_level(lint, lvl, src);
        }
    }

    fn visit_ids(&self, f: &fn(&mut ast_util::IdVisitor<Context>)) {
        let mut v = ast_util::IdVisitor {
            operation: self,
            pass_through_items: false,
            visited_outermost: false,
        };
        f(&mut v);
    }
}

pub fn each_lint(sess: session::Session,
                 attrs: &[ast::Attribute],
                 f: &fn(@ast::MetaItem, level, @str) -> bool) -> bool {
    let xs = [allow, warn, deny, forbid];
    for &level in xs.iter() {
        let level_name = level_to_str(level);
        for attr in attrs.iter().filter(|m| level_name == m.name()) {
            let meta = attr.node.value;
            let metas = match meta.node {
                ast::MetaList(_, ref metas) => metas,
                _ => {
                    sess.span_err(meta.span, "malformed lint attribute");
                    continue;
                }
            };
            for meta in metas.iter() {
                match meta.node {
                    ast::MetaWord(lintname) => {
                        if !f(*meta, level, lintname) {
                            return false;
                        }
                    }
                    _ => {
                        sess.span_err(meta.span, "malformed lint attribute");
                    }
                }
            }
        }
    }
    true
}

fn check_while_true_expr(cx: &Context, e: &ast::Expr) {
    match e.node {
        ast::ExprWhile(cond, _) => {
            match cond.node {
                ast::ExprLit(@codemap::Spanned {
                    node: ast::lit_bool(true), _}) =>
                {
                    cx.span_lint(while_true, e.span,
                                 "denote infinite loops with loop { ... }");
                }
                _ => ()
            }
        }
        _ => ()
    }
}

fn check_type_limits(cx: &Context, e: &ast::Expr) {
    return match e.node {
        ast::ExprBinary(_, binop, l, r) => {
            if is_comparison(binop) && !check_limits(cx.tcx, binop, l, r) {
                cx.span_lint(type_limits, e.span,
                             "comparison is useless due to type limits");
            }
        }
        _ => ()
    };

    fn is_valid<T:cmp::Ord>(binop: ast::BinOp, v: T,
                            min: T, max: T) -> bool {
        match binop {
            ast::BiLt => v <= max,
            ast::BiLe => v < max,
            ast::BiGt => v >= min,
            ast::BiGe => v > min,
            ast::BiEq | ast::BiNe => v >= min && v <= max,
            _ => fail2!()
        }
    }

    fn rev_binop(binop: ast::BinOp) -> ast::BinOp {
        match binop {
            ast::BiLt => ast::BiGt,
            ast::BiLe => ast::BiGe,
            ast::BiGt => ast::BiLt,
            ast::BiGe => ast::BiLe,
            _ => binop
        }
    }

    // for int & uint, be conservative with the warnings, so that the
    // warnings are consistent between 32- and 64-bit platforms
    fn int_ty_range(int_ty: ast::int_ty) -> (i64, i64) {
        match int_ty {
            ast::ty_i =>    (i64::min_value,        i64::max_value),
            ast::ty_i8 =>   (i8::min_value  as i64, i8::max_value  as i64),
            ast::ty_i16 =>  (i16::min_value as i64, i16::max_value as i64),
            ast::ty_i32 =>  (i32::min_value as i64, i32::max_value as i64),
            ast::ty_i64 =>  (i64::min_value,        i64::max_value)
        }
    }

    fn uint_ty_range(uint_ty: ast::uint_ty) -> (u64, u64) {
        match uint_ty {
            ast::ty_u =>   (u64::min_value,         u64::max_value),
            ast::ty_u8 =>  (u8::min_value   as u64, u8::max_value   as u64),
            ast::ty_u16 => (u16::min_value  as u64, u16::max_value  as u64),
            ast::ty_u32 => (u32::min_value  as u64, u32::max_value  as u64),
            ast::ty_u64 => (u64::min_value,         u64::max_value)
        }
    }

    fn check_limits(tcx: ty::ctxt, binop: ast::BinOp,
                    l: &ast::Expr, r: &ast::Expr) -> bool {
        let (lit, expr, swap) = match (&l.node, &r.node) {
            (&ast::ExprLit(_), _) => (l, r, true),
            (_, &ast::ExprLit(_)) => (r, l, false),
            _ => return true
        };
        // Normalize the binop so that the literal is always on the RHS in
        // the comparison
        let norm_binop = if swap { rev_binop(binop) } else { binop };
        match ty::get(ty::expr_ty(tcx, expr)).sty {
            ty::ty_int(int_ty) => {
                let (min, max) = int_ty_range(int_ty);
                let lit_val: i64 = match lit.node {
                    ast::ExprLit(li) => match li.node {
                        ast::lit_int(v, _) => v,
                        ast::lit_uint(v, _) => v as i64,
                        ast::lit_int_unsuffixed(v) => v,
                        _ => return true
                    },
                    _ => fail2!()
                };
                is_valid(norm_binop, lit_val, min, max)
            }
            ty::ty_uint(uint_ty) => {
                let (min, max): (u64, u64) = uint_ty_range(uint_ty);
                let lit_val: u64 = match lit.node {
                    ast::ExprLit(li) => match li.node {
                        ast::lit_int(v, _) => v as u64,
                        ast::lit_uint(v, _) => v,
                        ast::lit_int_unsuffixed(v) => v as u64,
                        _ => return true
                    },
                    _ => fail2!()
                };
                is_valid(norm_binop, lit_val, min, max)
            }
            _ => true
        }
    }

    fn is_comparison(binop: ast::BinOp) -> bool {
        match binop {
            ast::BiEq | ast::BiLt | ast::BiLe |
            ast::BiNe | ast::BiGe | ast::BiGt => true,
            _ => false
        }
    }
}

fn check_item_ctypes(cx: &Context, it: &ast::item) {
    fn check_ty(cx: &Context, ty: &ast::Ty) {
        match ty.node {
            ast::ty_path(_, _, id) => {
                match cx.tcx.def_map.get_copy(&id) {
                    ast::DefPrimTy(ast::ty_int(ast::ty_i)) => {
                        cx.span_lint(ctypes, ty.span,
                                "found rust type `int` in foreign module, while \
                                libc::c_int or libc::c_long should be used");
                    }
                    ast::DefPrimTy(ast::ty_uint(ast::ty_u)) => {
                        cx.span_lint(ctypes, ty.span,
                                "found rust type `uint` in foreign module, while \
                                libc::c_uint or libc::c_ulong should be used");
                    }
                    _ => ()
                }
            }
            ast::ty_ptr(ref mt) => { check_ty(cx, mt.ty) }
            _ => ()
        }
    }

    fn check_foreign_fn(cx: &Context, decl: &ast::fn_decl) {
        for input in decl.inputs.iter() {
            check_ty(cx, &input.ty);
        }
        check_ty(cx, &decl.output)
    }

    match it.node {
      ast::item_foreign_mod(ref nmod) if !nmod.abis.is_intrinsic() => {
        for ni in nmod.items.iter() {
            match ni.node {
                ast::foreign_item_fn(ref decl, _) => {
                    check_foreign_fn(cx, decl);
                }
                ast::foreign_item_static(ref t, _) => { check_ty(cx, t); }
            }
        }
      }
      _ => {/* nothing to do */ }
    }
}

fn check_heap_type(cx: &Context, span: Span, ty: ty::t) {
    let xs = [managed_heap_memory, owned_heap_memory, heap_memory];
    for &lint in xs.iter() {
        if cx.get_level(lint) == allow { continue }

        let mut n_box = 0;
        let mut n_uniq = 0;
        ty::fold_ty(cx.tcx, ty, |t| {
            match ty::get(t).sty {
              ty::ty_box(_) => n_box += 1,
              ty::ty_uniq(_) => n_uniq += 1,
              _ => ()
            };
            t
        });

        if n_uniq > 0 && lint != managed_heap_memory {
            let s = ty_to_str(cx.tcx, ty);
            let m = format!("type uses owned (~ type) pointers: {}", s);
            cx.span_lint(lint, span, m);
        }

        if n_box > 0 && lint != owned_heap_memory {
            let s = ty_to_str(cx.tcx, ty);
            let m = format!("type uses managed (@ type) pointers: {}", s);
            cx.span_lint(lint, span, m);
        }
    }
}

fn check_heap_item(cx: &Context, it: &ast::item) {
    match it.node {
        ast::item_fn(*) |
        ast::item_ty(*) |
        ast::item_enum(*) |
        ast::item_struct(*) => check_heap_type(cx, it.span,
                                               ty::node_id_to_type(cx.tcx,
                                                                   it.id)),
        _ => ()
    }

    // If it's a struct, we also have to check the fields' types
    match it.node {
        ast::item_struct(struct_def, _) => {
            for struct_field in struct_def.fields.iter() {
                check_heap_type(cx, struct_field.span,
                                ty::node_id_to_type(cx.tcx,
                                                    struct_field.node.id));
            }
        }
        _ => ()
    }
}

fn check_heap_expr(cx: &Context, e: &ast::Expr) {
    let ty = ty::expr_ty(cx.tcx, e);
    check_heap_type(cx, e.span, ty);
}

fn check_path_statement(cx: &Context, s: &ast::Stmt) {
    match s.node {
        ast::StmtSemi(@ast::Expr { node: ast::ExprPath(_), _ }, _) => {
            cx.span_lint(path_statement, s.span,
                         "path statement with no effect");
        }
        _ => ()
    }
}

fn check_item_non_camel_case_types(cx: &Context, it: &ast::item) {
    fn is_camel_case(cx: ty::ctxt, ident: ast::Ident) -> bool {
        let ident = cx.sess.str_of(ident);
        assert!(!ident.is_empty());
        let ident = ident.trim_chars(&'_');

        // start with a non-lowercase letter rather than non-uppercase
        // ones (some scripts don't have a concept of upper/lowercase)
        !ident.char_at(0).is_lowercase() &&
            !ident.contains_char('_')
    }

    fn check_case(cx: &Context, sort: &str, ident: ast::Ident, span: Span) {
        if !is_camel_case(cx.tcx, ident) {
            cx.span_lint(
                non_camel_case_types, span,
                format!("{} `{}` should have a camel case identifier",
                    sort, cx.tcx.sess.str_of(ident)));
        }
    }

    match it.node {
        ast::item_ty(*) | ast::item_struct(*) => {
            check_case(cx, "type", it.ident, it.span)
        }
        ast::item_trait(*) => {
            check_case(cx, "trait", it.ident, it.span)
        }
        ast::item_enum(ref enum_definition, _) => {
            check_case(cx, "type", it.ident, it.span);
            for variant in enum_definition.variants.iter() {
                check_case(cx, "variant", variant.node.name, variant.span);
            }
        }
        _ => ()
    }
}

fn check_item_non_uppercase_statics(cx: &Context, it: &ast::item) {
    match it.node {
        // only check static constants
        ast::item_static(_, ast::MutImmutable, _) => {
            let s = cx.tcx.sess.str_of(it.ident);
            // check for lowercase letters rather than non-uppercase
            // ones (some scripts don't have a concept of
            // upper/lowercase)
            if s.iter().any(|c| c.is_lowercase()) {
                cx.span_lint(non_uppercase_statics, it.span,
                             "static constant should have an uppercase identifier");
            }
        }
        _ => {}
    }
}

fn check_pat_non_uppercase_statics(cx: &Context, p: &ast::Pat) {
    // Lint for constants that look like binding identifiers (#7526)
    match (&p.node, cx.tcx.def_map.find(&p.id)) {
        (&ast::PatIdent(_, ref path, _), Some(&ast::DefStatic(_, false))) => {
            // last identifier alone is right choice for this lint.
            let ident = path.segments.last().identifier;
            let s = cx.tcx.sess.str_of(ident);
            if s.iter().any(|c| c.is_lowercase()) {
                cx.span_lint(non_uppercase_pattern_statics, path.span,
                             "static constant in pattern should be all caps");
            }
        }
        _ => {}
    }
}

fn check_unused_unsafe(cx: &Context, e: &ast::Expr) {
    match e.node {
        // Don't warn about generated blocks, that'll just pollute the
        // output.
        ast::ExprBlock(ref blk) => {
            if blk.rules == ast::UnsafeBlock(ast::UserProvided) &&
                !cx.tcx.used_unsafe.contains(&blk.id) {
                cx.span_lint(unused_unsafe, blk.span,
                             "unnecessary `unsafe` block");
            }
        }
        _ => ()
    }
}

fn check_unused_mut_pat(cx: &Context, p: @ast::Pat) {
    let mut used = false;
    let mut bindings = 0;
    do pat_util::pat_bindings(cx.tcx.def_map, p) |_, id, _, _| {
        used = used || cx.tcx.used_mut_nodes.contains(&id);
        bindings += 1;
    }
    if !used {
        let msg = if bindings == 1 {
            "variable does not need to be mutable"
        } else {
            "variables do not need to be mutable"
        };
        cx.span_lint(unused_mut, p.span, msg);
    }
}

fn check_unused_mut_fn_decl(cx: &Context, fd: &ast::fn_decl) {
    for arg in fd.inputs.iter() {
        if arg.is_mutbl {
            check_unused_mut_pat(cx, arg.pat);
        }
    }
}

fn check_unnecessary_allocation(cx: &Context, e: &ast::Expr) {
    // Warn if string and vector literals with sigils are immediately borrowed.
    // Those can have the sigil removed.
    match e.node {
        ast::ExprVstore(e2, ast::ExprVstoreUniq) |
        ast::ExprVstore(e2, ast::ExprVstoreBox) => {
            match e2.node {
                ast::ExprLit(@codemap::Spanned{node: ast::lit_str(*), _}) |
                ast::ExprVec(*) => {}
                _ => return
            }
        }

        _ => return
    }

    match cx.tcx.adjustments.find_copy(&e.id) {
        Some(@ty::AutoDerefRef(ty::AutoDerefRef {
            autoref: Some(ty::AutoBorrowVec(*)), _ })) => {
            cx.span_lint(unnecessary_allocation, e.span,
                         "unnecessary allocation, the sigil can be removed");
        }

        _ => ()
    }
}

struct MissingDocLintVisitor(ty::ctxt);

impl MissingDocLintVisitor {
    fn check_attrs(&self, attrs: &[ast::Attribute], id: ast::NodeId,
                   sp: Span, msg: ~str) {
        if !attrs.iter().any(|a| a.node.is_sugared_doc) {
            self.sess.add_lint(missing_doc, id, sp, msg);
        }
    }

    fn check_struct(&self, sdef: &ast::struct_def) {
        for field in sdef.fields.iter() {
            match field.node.kind {
                ast::named_field(_, vis) if vis != ast::private => {
                    self.check_attrs(field.node.attrs, field.node.id, field.span,
                                     ~"missing documentation for a field");
                }
                ast::unnamed_field | ast::named_field(*) => {}
            }
        }
    }

    fn doc_hidden(&self, attrs: &[ast::Attribute]) -> bool {
        do attrs.iter().any |attr| {
            "doc" == attr.name() &&
                match attr.meta_item_list() {
                    Some(l) => attr::contains_name(l, "hidden"),
                    None    => false // not of the form #[doc(...)]
                }
        }
    }
}

impl Visitor<()> for MissingDocLintVisitor {
    fn visit_ty_method(&mut self, m:&ast::TypeMethod, _: ()) {
        if self.doc_hidden(m.attrs) { return }

        // All ty_method objects are linted about because they're part of a
        // trait (no visibility)
        self.check_attrs(m.attrs, m.id, m.span,
                         ~"missing documentation for a method");
        visit::walk_ty_method(self, m, ());
    }

    fn visit_fn(&mut self, fk: &visit::fn_kind, d: &ast::fn_decl,
                b: &ast::Block, sp: Span, id: ast::NodeId, _: ()) {
        // Only warn about explicitly public methods.
        match *fk {
            visit::fk_method(_, _, m) => {
                if self.doc_hidden(m.attrs) {
                    return;
                }
                // If we're in a trait implementation, no need to duplicate
                // documentation
                if m.vis == ast::public {
                    self.check_attrs(m.attrs, id, sp,
                                     ~"missing documentation for a method");
                }
            }
            _ => {}
        }
        visit::walk_fn(self, fk, d, b, sp, id, ());
    }

    fn visit_item(&mut self, it: @ast::item, _: ()) {
        // If we're building a test harness, then warning about documentation is
        // probably not really relevant right now
        if self.sess.opts.test { return }
        if self.doc_hidden(it.attrs) { return }

        match it.node {
            ast::item_struct(sdef, _) if it.vis == ast::public => {
                self.check_attrs(it.attrs, it.id, it.span,
                                 ~"missing documentation for a struct");
                self.check_struct(sdef);
            }

            // Skip implementations because they inherit documentation from the
            // trait (which was already linted)
            ast::item_impl(_, Some(*), _, _) => return,

            ast::item_trait(*) if it.vis == ast::public => {
                self.check_attrs(it.attrs, it.id, it.span,
                                 ~"missing documentation for a trait");
            }

            ast::item_fn(*) if it.vis == ast::public => {
                self.check_attrs(it.attrs, it.id, it.span,
                                 ~"missing documentation for a function");
            }

            ast::item_enum(ref edef, _) if it.vis == ast::public => {
                self.check_attrs(it.attrs, it.id, it.span,
                                 ~"missing documentation for an enum");
                for variant in edef.variants.iter() {
                    if variant.node.vis == ast::private { continue; }

                    self.check_attrs(variant.node.attrs, variant.node.id,
                                     variant.span,
                                     ~"missing documentation for a variant");
                    match variant.node.kind {
                        ast::struct_variant_kind(sdef) => {
                            self.check_struct(sdef);
                        }
                        _ => ()
                    }
                }
            }

            _ => {}
        }
        visit::walk_item(self, it, ());
    }
}

/// Checks for use of items with #[deprecated], #[experimental] and
/// #[unstable] (or none of them) attributes.
fn check_stability(cx: &Context, e: &ast::Expr) {
    let def = match e.node {
        ast::ExprMethodCall(*) |
        ast::ExprPath(*) |
        ast::ExprStruct(*) => {
            match cx.tcx.def_map.find(&e.id) {
                Some(&def) => def,
                None => return
            }
        }
        _ => return
    };

    let id = ast_util::def_id_of_def(def);

    let stability = if ast_util::is_local(id) {
        // this crate
        match cx.tcx.items.find(&id.node) {
            Some(ast_node) => {
                let s = do ast_node.with_attrs |attrs| {
                    do attrs.map_move |a| {
                        attr::find_stability(a.iter().map(|a| a.meta()))
                    }
                };
                match s {
                    Some(s) => s,

                    // no possibility of having attributes
                    // (e.g. it's a local variable), so just
                    // ignore it.
                    None => return
                }
            }
            _ => cx.tcx.sess.bug(format!("handle_def: {:?} not found", id))
        }
    } else {
        // cross-crate

        let mut s = None;
        // run through all the attributes and take the first
        // stability one.
        do csearch::get_item_attrs(cx.tcx.cstore, id) |meta_items| {
            if s.is_none() {
                s = attr::find_stability(meta_items.move_iter())
            }
        }
        s
    };

    let (lint, label) = match stability {
        // no stability attributes == Unstable
        None => (unstable, "unmarked"),
        Some(attr::Stability { level: attr::Unstable, _ }) =>
                (unstable, "unstable"),
        Some(attr::Stability { level: attr::Experimental, _ }) =>
                (experimental, "experimental"),
        Some(attr::Stability { level: attr::Deprecated, _ }) =>
                (deprecated, "deprecated"),
        _ => return
    };

    let msg = match stability {
        Some(attr::Stability { text: Some(ref s), _ }) => {
            format!("use of {} item: {}", label, *s)
        }
        _ => format!("use of {} item", label)
    };

    cx.span_lint(lint, e.span, msg);
}

impl Visitor<()> for Context {
    fn visit_item(&mut self, it: @ast::item, _: ()) {
        do self.with_lint_attrs(it.attrs) |cx| {
            check_item_ctypes(cx, it);
            check_item_non_camel_case_types(cx, it);
            check_item_non_uppercase_statics(cx, it);
            check_heap_item(cx, it);

            do cx.visit_ids |v| {
                v.visit_item(it, ());
            }

            visit::walk_item(cx, it, ());
        }
    }

    fn visit_pat(&mut self, p: @ast::Pat, _: ()) {
        check_pat_non_uppercase_statics(self, p);
        visit::walk_pat(self, p, ());
    }

    fn visit_expr(&mut self, e: @ast::Expr, _: ()) {
        check_while_true_expr(self, e);
        check_stability(self, e);
        check_unused_unsafe(self, e);
        check_unnecessary_allocation(self, e);
        check_heap_expr(self, e);
        check_type_limits(self, e);

        visit::walk_expr(self, e, ());
    }

    fn visit_stmt(&mut self, s: @ast::Stmt, _: ()) {
        check_path_statement(self, s);

        visit::walk_stmt(self, s, ());
    }

    fn visit_ty_method(&mut self, tm: &ast::TypeMethod, _: ()) {
        check_unused_mut_fn_decl(self, &tm.decl);
        visit::walk_ty_method(self, tm, ());
    }

    fn visit_trait_method(&mut self, tm: &ast::trait_method, _: ()) {
        match *tm {
            ast::required(ref m) => check_unused_mut_fn_decl(self, &m.decl),
            ast::provided(ref m) => check_unused_mut_fn_decl(self, &m.decl)
        }
        visit::walk_trait_method(self, tm, ());
    }

    fn visit_local(&mut self, l: @ast::Local, _: ()) {
        if l.is_mutbl {
            check_unused_mut_pat(self, l.pat);
        }
        visit::walk_local(self, l, ());
    }

    fn visit_fn(&mut self, fk: &visit::fn_kind, decl: &ast::fn_decl,
                body: &ast::Block, span: Span, id: ast::NodeId, _: ()) {
        let recurse = |this: &mut Context| {
            check_unused_mut_fn_decl(this, decl);
            visit::walk_fn(this, fk, decl, body, span, id, ());
        };

        match *fk {
            visit::fk_method(_, _, m) => {
                do self.with_lint_attrs(m.attrs) |cx| {
                    do cx.visit_ids |v| {
                        v.visit_fn(fk, decl, body, span, id, ());
                    }
                    recurse(cx);
                }
            }
            _ => recurse(self),
        }
    }
}

impl ast_util::IdVisitingOperation for Context {
    fn visit_id(&self, id: ast::NodeId) {
        match self.tcx.sess.lints.pop(&id) {
            None => {}
            Some(l) => {
                for (lint, span, msg) in l.move_iter() {
                    self.span_lint(lint, span, msg)
                }
            }
        }
    }
}

pub fn check_crate(tcx: ty::ctxt, crate: &ast::Crate) {
    // This visitor contains more state than is currently maintained in Context,
    // and there's no reason for the Context to keep track of this information
    // really
    let mut dox = MissingDocLintVisitor(tcx);
    visit::walk_crate(&mut dox, crate, ());

    let mut cx = Context {
        dict: @get_lint_dict(),
        cur: SmallIntMap::new(),
        tcx: tcx,
        lint_stack: ~[],
    };

    // Install default lint levels, followed by the command line levels, and
    // then actually visit the whole crate.
    for (_, spec) in cx.dict.iter() {
        cx.set_level(spec.lint, spec.default, Default);
    }
    for &(lint, level) in tcx.sess.opts.lint_opts.iter() {
        cx.set_level(lint, level, CommandLine);
    }
    do cx.with_lint_attrs(crate.attrs) |cx| {
        do cx.visit_ids |v| {
            v.visited_outermost = true;
            visit::walk_crate(v, crate, ());
        }
        visit::walk_crate(cx, crate, ());
    }

    // If we missed any lints added to the session, then there's a bug somewhere
    // in the iteration code.
    for (id, v) in tcx.sess.lints.iter() {
        for &(lint, span, ref msg) in v.iter() {
            tcx.sess.span_bug(span, format!("unprocessed lint {:?} at {}: {}",
                                            lint,
                                            ast_map::node_id_to_str(tcx.items,
                                                *id,
                                                token::get_ident_interner()),
                                            *msg))
        }
    }

    tcx.sess.abort_if_errors();
}
