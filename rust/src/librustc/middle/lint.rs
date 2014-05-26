// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
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

#![allow(non_camel_case_types)]

use driver::session;
use metadata::csearch;
use middle::dead::DEAD_CODE_LINT_STR;
use middle::pat_util;
use middle::privacy;
use middle::trans::adt; // for `adt::is_ffi_safe`
use middle::ty;
use middle::typeck::astconv::{ast_ty_to_ty, AstConv};
use middle::typeck::infer;
use middle::typeck;
use util::ppaux::{ty_to_str};
use util::nodemap::NodeSet;

use std::cmp;
use collections::HashMap;
use std::i16;
use std::i32;
use std::i64;
use std::i8;
use std::rc::Rc;
use std::to_str::ToStr;
use std::u16;
use std::u32;
use std::u64;
use std::u8;
use collections::SmallIntMap;
use syntax::abi;
use syntax::ast_map;
use syntax::ast_util::IdVisitingOperation;
use syntax::attr::AttrMetaMethods;
use syntax::attr;
use syntax::codemap::Span;
use syntax::parse::token::InternedString;
use syntax::parse::token;
use syntax::visit::Visitor;
use syntax::{ast, ast_util, visit};

#[deriving(Clone, Show, Eq, Ord, TotalEq, TotalOrd, Hash)]
pub enum Lint {
    CTypes,
    UnusedImports,
    UnnecessaryQualification,
    WhileTrue,
    PathStatement,
    UnrecognizedLint,
    NonCamelCaseTypes,
    NonUppercaseStatics,
    NonUppercasePatternStatics,
    UppercaseVariables,
    UnnecessaryParens,
    TypeLimits,
    TypeOverflow,
    UnusedUnsafe,
    UnsafeBlock,
    AttributeUsage,
    UnusedAttribute,
    UnknownFeatures,
    UnknownCrateType,
    UnsignedNegate,
    VariantSizeDifference,

    ManagedHeapMemory,
    OwnedHeapMemory,
    HeapMemory,

    UnusedVariable,
    DeadAssignment,
    UnusedMut,
    UnnecessaryAllocation,
    DeadCode,
    VisiblePrivateTypes,
    UnnecessaryTypecast,

    MissingDoc,
    UnreachableCode,

    Deprecated,
    Experimental,
    Unstable,

    UnusedMustUse,
    UnusedResult,

    DeprecatedOwnedVector,

    Warnings,

    RawPointerDeriving,
}

pub fn level_to_str(lv: Level) -> &'static str {
    match lv {
      Allow => "allow",
      Warn => "warn",
      Deny => "deny",
      Forbid => "forbid"
    }
}

#[deriving(Clone, Eq, Ord, TotalEq, TotalOrd)]
pub enum Level {
    Allow, Warn, Deny, Forbid
}

#[deriving(Clone, Eq, Ord, TotalEq, TotalOrd)]
pub struct LintSpec {
    pub default: Level,
    pub lint: Lint,
    pub desc: &'static str,
}

pub type LintDict = HashMap<&'static str, LintSpec>;

// this is public for the lints that run in trans
#[deriving(Eq)]
pub enum LintSource {
    Node(Span),
    Default,
    CommandLine
}

static lint_table: &'static [(&'static str, LintSpec)] = &[
    ("ctypes",
     LintSpec {
        lint: CTypes,
        desc: "proper use of libc types in foreign modules",
        default: Warn
     }),

    ("unused_imports",
     LintSpec {
        lint: UnusedImports,
        desc: "imports that are never used",
        default: Warn
     }),

    ("unnecessary_qualification",
     LintSpec {
        lint: UnnecessaryQualification,
        desc: "detects unnecessarily qualified names",
        default: Allow
     }),

    ("while_true",
     LintSpec {
        lint: WhileTrue,
        desc: "suggest using `loop { }` instead of `while true { }`",
        default: Warn
     }),

    ("path_statement",
     LintSpec {
        lint: PathStatement,
        desc: "path statements with no effect",
        default: Warn
     }),

    ("unrecognized_lint",
     LintSpec {
        lint: UnrecognizedLint,
        desc: "unrecognized lint attribute",
        default: Warn
     }),

    ("non_camel_case_types",
     LintSpec {
        lint: NonCamelCaseTypes,
        desc: "types, variants and traits should have camel case names",
        default: Warn
     }),

    ("non_uppercase_statics",
     LintSpec {
         lint: NonUppercaseStatics,
         desc: "static constants should have uppercase identifiers",
         default: Allow
     }),

    ("non_uppercase_pattern_statics",
     LintSpec {
         lint: NonUppercasePatternStatics,
         desc: "static constants in match patterns should be all caps",
         default: Warn
     }),

    ("uppercase_variables",
     LintSpec {
         lint: UppercaseVariables,
         desc: "variable and structure field names should start with a lowercase character",
         default: Warn
     }),

     ("unnecessary_parens",
     LintSpec {
        lint: UnnecessaryParens,
        desc: "`if`, `match`, `while` and `return` do not need parentheses",
        default: Warn
     }),

    ("managed_heap_memory",
     LintSpec {
        lint: ManagedHeapMemory,
        desc: "use of managed (@ type) heap memory",
        default: Allow
     }),

    ("owned_heap_memory",
     LintSpec {
        lint: OwnedHeapMemory,
        desc: "use of owned (Box type) heap memory",
        default: Allow
     }),

    ("heap_memory",
     LintSpec {
        lint: HeapMemory,
        desc: "use of any (Box type or @ type) heap memory",
        default: Allow
     }),

    ("type_limits",
     LintSpec {
        lint: TypeLimits,
        desc: "comparisons made useless by limits of the types involved",
        default: Warn
     }),

    ("type_overflow",
     LintSpec {
        lint: TypeOverflow,
        desc: "literal out of range for its type",
        default: Warn
     }),


    ("unused_unsafe",
     LintSpec {
        lint: UnusedUnsafe,
        desc: "unnecessary use of an `unsafe` block",
        default: Warn
    }),

    ("unsafe_block",
     LintSpec {
        lint: UnsafeBlock,
        desc: "usage of an `unsafe` block",
        default: Allow
    }),

    ("attribute_usage",
     LintSpec {
        lint: AttributeUsage,
        desc: "detects bad use of attributes",
        default: Warn
    }),

    ("unused_attribute",
     LintSpec {
         lint: UnusedAttribute,
         desc: "detects attributes that were not used by the compiler",
         default: Warn
    }),

    ("unused_variable",
     LintSpec {
        lint: UnusedVariable,
        desc: "detect variables which are not used in any way",
        default: Warn
    }),

    ("dead_assignment",
     LintSpec {
        lint: DeadAssignment,
        desc: "detect assignments that will never be read",
        default: Warn
    }),

    ("unnecessary_typecast",
     LintSpec {
        lint: UnnecessaryTypecast,
        desc: "detects unnecessary type casts, that can be removed",
        default: Allow,
    }),

    ("unused_mut",
     LintSpec {
        lint: UnusedMut,
        desc: "detect mut variables which don't need to be mutable",
        default: Warn
    }),

    ("unnecessary_allocation",
     LintSpec {
        lint: UnnecessaryAllocation,
        desc: "detects unnecessary allocations that can be eliminated",
        default: Warn
    }),

    (DEAD_CODE_LINT_STR,
     LintSpec {
        lint: DeadCode,
        desc: "detect piece of code that will never be used",
        default: Warn
    }),
    ("visible_private_types",
     LintSpec {
        lint: VisiblePrivateTypes,
        desc: "detect use of private types in exported type signatures",
        default: Warn
    }),

    ("missing_doc",
     LintSpec {
        lint: MissingDoc,
        desc: "detects missing documentation for public members",
        default: Allow
    }),

    ("unreachable_code",
     LintSpec {
        lint: UnreachableCode,
        desc: "detects unreachable code",
        default: Warn
    }),

    ("deprecated",
     LintSpec {
        lint: Deprecated,
        desc: "detects use of #[deprecated] items",
        default: Warn
    }),

    ("experimental",
     LintSpec {
        lint: Experimental,
        desc: "detects use of #[experimental] items",
        default: Warn
    }),

    ("unstable",
     LintSpec {
        lint: Unstable,
        desc: "detects use of #[unstable] items (incl. items with no stability attribute)",
        default: Allow
    }),

    ("warnings",
     LintSpec {
        lint: Warnings,
        desc: "mass-change the level for lints which produce warnings",
        default: Warn
    }),

    ("unknown_features",
     LintSpec {
        lint: UnknownFeatures,
        desc: "unknown features found in crate-level #[feature] directives",
        default: Deny,
    }),

    ("unknown_crate_type",
    LintSpec {
        lint: UnknownCrateType,
        desc: "unknown crate type found in #[crate_type] directive",
        default: Deny,
    }),

    ("unsigned_negate",
    LintSpec {
        lint: UnsignedNegate,
        desc: "using an unary minus operator on unsigned type",
        default: Warn
    }),

    ("variant_size_difference",
    LintSpec {
        lint: VariantSizeDifference,
        desc: "detects enums with widely varying variant sizes",
        default: Allow,
    }),

    ("unused_must_use",
    LintSpec {
        lint: UnusedMustUse,
        desc: "unused result of a type flagged as #[must_use]",
        default: Warn,
    }),

    ("unused_result",
    LintSpec {
        lint: UnusedResult,
        desc: "unused result of an expression in a statement",
        default: Allow,
    }),

    ("deprecated_owned_vector",
     LintSpec {
        lint: DeprecatedOwnedVector,
        desc: "use of a `~[T]` vector",
        default: Allow,
    }),

    ("raw_pointer_deriving",
     LintSpec {
        lint: RawPointerDeriving,
        desc: "uses of #[deriving] with raw pointers are rarely correct",
        default: Warn,
    }),
];

/*
  Pass names should not contain a '-', as the compiler normalizes
  '-' to '_' in command-line flags
 */
pub fn get_lint_dict() -> LintDict {
    lint_table.iter().map(|&(k, v)| (k, v)).collect()
}

struct Context<'a> {
    /// All known lint modes (string versions)
    dict: LintDict,
    /// Current levels of each lint warning
    cur: SmallIntMap<(Level, LintSource)>,
    /// Context we're checking in (used to access fields like sess)
    tcx: &'a ty::ctxt,
    /// Items exported by the crate; used by the missing_doc lint.
    exported_items: &'a privacy::ExportedItems,
    /// The id of the current `ast::StructDef` being walked.
    cur_struct_def_id: ast::NodeId,
    /// Whether some ancestor of the current node was marked
    /// #[doc(hidden)].
    is_doc_hidden: bool,

    /// When recursing into an attributed node of the ast which modifies lint
    /// levels, this stack keeps track of the previous lint levels of whatever
    /// was modified.
    lint_stack: Vec<(Lint, Level, LintSource)>,

    /// Id of the last visited negated expression
    negated_expr_id: ast::NodeId,

    /// Ids of structs/enums which have been checked for raw_pointer_deriving
    checked_raw_pointers: NodeSet,

    /// Level of lints for certain NodeIds, stored here because the body of
    /// the lint needs to run in trans.
    node_levels: HashMap<(ast::NodeId, Lint), (Level, LintSource)>,
}

pub fn emit_lint(level: Level, src: LintSource, msg: &str, span: Span,
                 lint_str: &str, tcx: &ty::ctxt) {
    if level == Allow { return }

    let mut note = None;
    let msg = match src {
        Default => {
            format!("{}, \\#[{}({})] on by default", msg,
                level_to_str(level), lint_str)
        },
        CommandLine => {
            format!("{} [-{} {}]", msg,
                match level {
                    Warn => 'W', Deny => 'D', Forbid => 'F',
                    Allow => fail!()
                }, lint_str.replace("_", "-"))
        },
        Node(src) => {
            note = Some(src);
            msg.to_str()
        }
    };

    match level {
        Warn =>          { tcx.sess.span_warn(span, msg.as_slice()); }
        Deny | Forbid => { tcx.sess.span_err(span, msg.as_slice());  }
        Allow => fail!(),
    }

    for &span in note.iter() {
        tcx.sess.span_note(span, "lint level defined here");
    }
}

pub fn lint_to_str(lint: Lint) -> &'static str {
    for &(name, lspec) in lint_table.iter() {
        if lspec.lint == lint {
            return name;
        }
    }

    fail!("unrecognized lint: {}", lint);
}

impl<'a> Context<'a> {
    fn get_level(&self, lint: Lint) -> Level {
        match self.cur.find(&(lint as uint)) {
          Some(&(lvl, _)) => lvl,
          None => Allow
        }
    }

    fn get_source(&self, lint: Lint) -> LintSource {
        match self.cur.find(&(lint as uint)) {
          Some(&(_, src)) => src,
          None => Default
        }
    }

    fn set_level(&mut self, lint: Lint, level: Level, src: LintSource) {
        if level == Allow {
            self.cur.remove(&(lint as uint));
        } else {
            self.cur.insert(lint as uint, (level, src));
        }
    }

    fn lint_to_str(&self, lint: Lint) -> &'static str {
        for (k, v) in self.dict.iter() {
            if v.lint == lint {
                return *k;
            }
        }
        fail!("unregistered lint {}", lint);
    }

    fn span_lint(&self, lint: Lint, span: Span, msg: &str) {
        let (level, src) = match self.cur.find(&(lint as uint)) {
            None => { return }
            Some(&(Warn, src)) => (self.get_level(Warnings), src),
            Some(&pair) => pair,
        };

        emit_lint(level, src, msg, span, self.lint_to_str(lint), self.tcx);
    }

    /**
     * Merge the lints specified by any lint attributes into the
     * current lint context, call the provided function, then reset the
     * lints in effect to their previous state.
     */
    fn with_lint_attrs(&mut self,
                       attrs: &[ast::Attribute],
                       f: |&mut Context|) {
        // Parse all of the lint attributes, and then add them all to the
        // current dictionary of lint information. Along the way, keep a history
        // of what we changed so we can roll everything back after invoking the
        // specified closure
        let mut pushed = 0u;
        each_lint(&self.tcx.sess, attrs, |meta, level, lintname| {
            match self.dict.find_equiv(&lintname) {
                None => {
                    self.span_lint(
                        UnrecognizedLint,
                        meta.span,
                        format!("unknown `{}` attribute: `{}`",
                                level_to_str(level), lintname).as_slice());
                }
                Some(lint) => {
                    let lint = lint.lint;
                    let now = self.get_level(lint);
                    if now == Forbid && level != Forbid {
                        self.tcx.sess.span_err(meta.span,
                        format!("{}({}) overruled by outer forbid({})",
                                level_to_str(level),
                                lintname,
                                lintname).as_slice());
                    } else if now != level {
                        let src = self.get_source(lint);
                        self.lint_stack.push((lint, now, src));
                        pushed += 1;
                        self.set_level(lint, level, Node(meta.span));
                    }
                }
            }
            true
        });

        let old_is_doc_hidden = self.is_doc_hidden;
        self.is_doc_hidden =
            self.is_doc_hidden ||
            attrs.iter()
                 .any(|attr| {
                     attr.name().equiv(&("doc")) &&
                     match attr.meta_item_list() {
                         None => false,
                         Some(l) => {
                             attr::contains_name(l.as_slice(), "hidden")
                         }
                     }
                 });

        f(self);

        // rollback
        self.is_doc_hidden = old_is_doc_hidden;
        for _ in range(0, pushed) {
            let (lint, lvl, src) = self.lint_stack.pop().unwrap();
            self.set_level(lint, lvl, src);
        }
    }

    fn visit_ids(&self, f: |&mut ast_util::IdVisitor<Context>|) {
        let mut v = ast_util::IdVisitor {
            operation: self,
            pass_through_items: false,
            visited_outermost: false,
        };
        f(&mut v);
    }
}

/// Check that every lint from the list of attributes satisfies `f`.
/// Return true if that's the case. Otherwise return false.
pub fn each_lint(sess: &session::Session,
                 attrs: &[ast::Attribute],
                 f: |@ast::MetaItem, Level, InternedString| -> bool)
                 -> bool {
    let xs = [Allow, Warn, Deny, Forbid];
    for &level in xs.iter() {
        let level_name = level_to_str(level);
        for attr in attrs.iter().filter(|m| m.check_name(level_name)) {
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
                    ast::MetaWord(ref lintname) => {
                        if !f(*meta, level, (*lintname).clone()) {
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

/// Check from a list of attributes if it contains the appropriate
/// `#[level(lintname)]` attribute (e.g. `#[allow(dead_code)]).
pub fn contains_lint(attrs: &[ast::Attribute],
                     level: Level,
                     lintname: &'static str)
                     -> bool {
    let level_name = level_to_str(level);
    for attr in attrs.iter().filter(|m| m.name().equiv(&level_name)) {
        if attr.meta_item_list().is_none() {
            continue
        }
        let list = attr.meta_item_list().unwrap();
        for meta_item in list.iter() {
            if meta_item.name().equiv(&lintname) {
                return true;
            }
        }
    }
    false
}

fn check_while_true_expr(cx: &Context, e: &ast::Expr) {
    match e.node {
        ast::ExprWhile(cond, _) => {
            match cond.node {
                ast::ExprLit(lit) => {
                    match lit.node {
                        ast::LitBool(true) => {
                            cx.span_lint(WhileTrue,
                                         e.span,
                                         "denote infinite loops with loop \
                                          { ... }");
                        }
                        _ => {}
                    }
                }
                _ => ()
            }
        }
        _ => ()
    }
}
impl<'a> AstConv for Context<'a>{
    fn tcx<'a>(&'a self) -> &'a ty::ctxt { self.tcx }

    fn get_item_ty(&self, id: ast::DefId) -> ty::ty_param_bounds_and_ty {
        ty::lookup_item_type(self.tcx, id)
    }

    fn get_trait_def(&self, id: ast::DefId) -> Rc<ty::TraitDef> {
        ty::lookup_trait_def(self.tcx, id)
    }

    fn ty_infer(&self, _span: Span) -> ty::t {
        infer::new_infer_ctxt(self.tcx).next_ty_var()
    }
}


fn check_unused_casts(cx: &Context, e: &ast::Expr) {
    return match e.node {
        ast::ExprCast(expr, ty) => {
            let t_t = ast_ty_to_ty(cx, &infer::new_infer_ctxt(cx.tcx), ty);
            if  ty::get(ty::expr_ty(cx.tcx, expr)).sty == ty::get(t_t).sty {
                cx.span_lint(UnnecessaryTypecast, ty.span,
                             "unnecessary type cast");
            }
        }
        _ => ()
    };
}

fn check_type_limits(cx: &Context, e: &ast::Expr) {
    return match e.node {
        ast::ExprUnary(ast::UnNeg, ex) => {
            match ex.node  {
                ast::ExprLit(lit) => {
                    match lit.node {
                        ast::LitUint(..) => {
                            cx.span_lint(UnsignedNegate, e.span,
                                         "negation of unsigned int literal may be unintentional");
                        },
                        _ => ()
                    }
                },
                _ => {
                    let t = ty::expr_ty(cx.tcx, ex);
                    match ty::get(t).sty {
                        ty::ty_uint(_) => {
                            cx.span_lint(UnsignedNegate, e.span,
                                         "negation of unsigned int variable may be unintentional");
                        },
                        _ => ()
                    }
                }
            }
        },
        ast::ExprBinary(binop, l, r) => {
            if is_comparison(binop) && !check_limits(cx.tcx, binop, l, r) {
                cx.span_lint(TypeLimits, e.span,
                             "comparison is useless due to type limits");
            }
        },
        ast::ExprLit(lit) => {
            match ty::get(ty::expr_ty(cx.tcx, e)).sty {
                ty::ty_int(t) => {
                    let int_type = if t == ast::TyI {
                        cx.tcx.sess.targ_cfg.int_type
                    } else { t };
                    let (min, max) = int_ty_range(int_type);
                    let mut lit_val: i64 = match lit.node {
                        ast::LitInt(v, _) => v,
                        ast::LitUint(v, _) => v as i64,
                        ast::LitIntUnsuffixed(v) => v,
                        _ => fail!()
                    };
                    if cx.negated_expr_id == e.id {
                        lit_val *= -1;
                    }
                    if  lit_val < min || lit_val > max {
                        cx.span_lint(TypeOverflow, e.span,
                                     "literal out of range for its type");
                    }
                },
                ty::ty_uint(t) => {
                    let uint_type = if t == ast::TyU {
                        cx.tcx.sess.targ_cfg.uint_type
                    } else { t };
                    let (min, max) = uint_ty_range(uint_type);
                    let lit_val: u64 = match lit.node {
                        ast::LitInt(v, _) => v as u64,
                        ast::LitUint(v, _) => v,
                        ast::LitIntUnsuffixed(v) => v as u64,
                        _ => fail!()
                    };
                    if  lit_val < min || lit_val > max {
                        cx.span_lint(TypeOverflow, e.span,
                                     "literal out of range for its type");
                    }
                },

                _ => ()
            };
        },
        _ => ()
    };

    fn is_valid<T:cmp::Ord>(binop: ast::BinOp, v: T,
                            min: T, max: T) -> bool {
        match binop {
            ast::BiLt => v >  min && v <= max,
            ast::BiLe => v >= min && v <  max,
            ast::BiGt => v >= min && v <  max,
            ast::BiGe => v >  min && v <= max,
            ast::BiEq | ast::BiNe => v >= min && v <= max,
            _ => fail!()
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
    fn int_ty_range(int_ty: ast::IntTy) -> (i64, i64) {
        match int_ty {
            ast::TyI =>    (i64::MIN,        i64::MAX),
            ast::TyI8 =>   (i8::MIN  as i64, i8::MAX  as i64),
            ast::TyI16 =>  (i16::MIN as i64, i16::MAX as i64),
            ast::TyI32 =>  (i32::MIN as i64, i32::MAX as i64),
            ast::TyI64 =>  (i64::MIN,        i64::MAX)
        }
    }

    fn uint_ty_range(uint_ty: ast::UintTy) -> (u64, u64) {
        match uint_ty {
            ast::TyU =>   (u64::MIN,         u64::MAX),
            ast::TyU8 =>  (u8::MIN   as u64, u8::MAX   as u64),
            ast::TyU16 => (u16::MIN  as u64, u16::MAX  as u64),
            ast::TyU32 => (u32::MIN  as u64, u32::MAX  as u64),
            ast::TyU64 => (u64::MIN,         u64::MAX)
        }
    }

    fn check_limits(tcx: &ty::ctxt, binop: ast::BinOp,
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
                        ast::LitInt(v, _) => v,
                        ast::LitUint(v, _) => v as i64,
                        ast::LitIntUnsuffixed(v) => v,
                        _ => return true
                    },
                    _ => fail!()
                };
                is_valid(norm_binop, lit_val, min, max)
            }
            ty::ty_uint(uint_ty) => {
                let (min, max): (u64, u64) = uint_ty_range(uint_ty);
                let lit_val: u64 = match lit.node {
                    ast::ExprLit(li) => match li.node {
                        ast::LitInt(v, _) => v as u64,
                        ast::LitUint(v, _) => v,
                        ast::LitIntUnsuffixed(v) => v as u64,
                        _ => return true
                    },
                    _ => fail!()
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

fn check_item_ctypes(cx: &Context, it: &ast::Item) {
    fn check_ty(cx: &Context, ty: &ast::Ty) {
        match ty.node {
            ast::TyPath(_, _, id) => {
                match cx.tcx.def_map.borrow().get_copy(&id) {
                    ast::DefPrimTy(ast::TyInt(ast::TyI)) => {
                        cx.span_lint(CTypes, ty.span,
                                "found rust type `int` in foreign module, while \
                                libc::c_int or libc::c_long should be used");
                    }
                    ast::DefPrimTy(ast::TyUint(ast::TyU)) => {
                        cx.span_lint(CTypes, ty.span,
                                "found rust type `uint` in foreign module, while \
                                libc::c_uint or libc::c_ulong should be used");
                    }
                    ast::DefTy(def_id) => {
                        if !adt::is_ffi_safe(cx.tcx, def_id) {
                            cx.span_lint(CTypes, ty.span,
                                         "found enum type without foreign-function-safe \
                                          representation annotation in foreign module");
                            // hmm... this message could be more helpful
                        }
                    }
                    _ => ()
                }
            }
            ast::TyPtr(ref mt) => { check_ty(cx, mt.ty) }
            _ => {}
        }
    }

    fn check_foreign_fn(cx: &Context, decl: &ast::FnDecl) {
        for input in decl.inputs.iter() {
            check_ty(cx, input.ty);
        }
        check_ty(cx, decl.output)
    }

    match it.node {
      ast::ItemForeignMod(ref nmod) if nmod.abi != abi::RustIntrinsic => {
        for ni in nmod.items.iter() {
            match ni.node {
                ast::ForeignItemFn(decl, _) => check_foreign_fn(cx, decl),
                ast::ForeignItemStatic(t, _) => check_ty(cx, t)
            }
        }
      }
      _ => {/* nothing to do */ }
    }
}

fn check_heap_type(cx: &Context, span: Span, ty: ty::t) {
    let xs = [ManagedHeapMemory, OwnedHeapMemory, HeapMemory];
    for &lint in xs.iter() {
        if cx.get_level(lint) == Allow { continue }

        let mut n_box = 0;
        let mut n_uniq = 0;
        ty::fold_ty(cx.tcx, ty, |t| {
            match ty::get(t).sty {
                ty::ty_box(_) => {
                    n_box += 1;
                }
                ty::ty_uniq(_) |
                ty::ty_trait(box ty::TyTrait {
                    store: ty::UniqTraitStore, ..
                }) |
                ty::ty_closure(box ty::ClosureTy {
                    store: ty::UniqTraitStore,
                    ..
                }) => {
                    n_uniq += 1;
                }

                _ => ()
            };
            t
        });

        if n_uniq > 0 && lint != ManagedHeapMemory {
            let s = ty_to_str(cx.tcx, ty);
            let m = format!("type uses owned (Box type) pointers: {}", s);
            cx.span_lint(lint, span, m.as_slice());
        }

        if n_box > 0 && lint != OwnedHeapMemory {
            let s = ty_to_str(cx.tcx, ty);
            let m = format!("type uses managed (@ type) pointers: {}", s);
            cx.span_lint(lint, span, m.as_slice());
        }
    }
}

fn check_heap_item(cx: &Context, it: &ast::Item) {
    match it.node {
        ast::ItemFn(..) |
        ast::ItemTy(..) |
        ast::ItemEnum(..) |
        ast::ItemStruct(..) => check_heap_type(cx, it.span,
                                               ty::node_id_to_type(cx.tcx,
                                                                   it.id)),
        _ => ()
    }

    // If it's a struct, we also have to check the fields' types
    match it.node {
        ast::ItemStruct(struct_def, _) => {
            for struct_field in struct_def.fields.iter() {
                check_heap_type(cx, struct_field.span,
                                ty::node_id_to_type(cx.tcx,
                                                    struct_field.node.id));
            }
        }
        _ => ()
    }
}

struct RawPtrDerivingVisitor<'a> {
    cx: &'a Context<'a>
}

impl<'a> Visitor<()> for RawPtrDerivingVisitor<'a> {
    fn visit_ty(&mut self, ty: &ast::Ty, _: ()) {
        static MSG: &'static str = "use of `#[deriving]` with a raw pointer";
        match ty.node {
            ast::TyPtr(..) => self.cx.span_lint(RawPointerDeriving, ty.span, MSG),
            _ => {}
        }
        visit::walk_ty(self, ty, ());
    }
    // explicit override to a no-op to reduce code bloat
    fn visit_expr(&mut self, _: &ast::Expr, _: ()) {}
    fn visit_block(&mut self, _: &ast::Block, _: ()) {}
}

fn check_raw_ptr_deriving(cx: &mut Context, item: &ast::Item) {
    if !attr::contains_name(item.attrs.as_slice(), "automatically_derived") {
        return
    }
    let did = match item.node {
        ast::ItemImpl(..) => {
            match ty::get(ty::node_id_to_type(cx.tcx, item.id)).sty {
                ty::ty_enum(did, _) => did,
                ty::ty_struct(did, _) => did,
                _ => return,
            }
        }
        _ => return,
    };
    if !ast_util::is_local(did) { return }
    let item = match cx.tcx.map.find(did.node) {
        Some(ast_map::NodeItem(item)) => item,
        _ => return,
    };
    if !cx.checked_raw_pointers.insert(item.id) { return }
    match item.node {
        ast::ItemStruct(..) | ast::ItemEnum(..) => {
            let mut visitor = RawPtrDerivingVisitor { cx: cx };
            visit::walk_item(&mut visitor, item, ());
        }
        _ => {}
    }
}

static crate_attrs: &'static [&'static str] = &[
    "crate_type", "feature", "no_start", "no_main", "no_std", "crate_id",
    "desc", "comment", "license", "copyright", // not used in rustc now
    "no_builtins",
];


static obsolete_attrs: &'static [(&'static str, &'static str)] = &[
    ("abi", "Use `extern \"abi\" fn` instead"),
    ("auto_encode", "Use `#[deriving(Encodable)]` instead"),
    ("auto_decode", "Use `#[deriving(Decodable)]` instead"),
    ("fast_ffi", "Remove it"),
    ("fixed_stack_segment", "Remove it"),
    ("rust_stack", "Remove it"),
];

static other_attrs: &'static [&'static str] = &[
    // item-level
    "address_insignificant", // can be crate-level too
    "thread_local", // for statics
    "allow", "deny", "forbid", "warn", // lint options
    "deprecated", "experimental", "unstable", "stable", "locked", "frozen", //item stability
    "cfg", "doc", "export_name", "link_section",
    "no_mangle", "static_assert", "unsafe_no_drop_flag", "packed",
    "simd", "repr", "deriving", "unsafe_destructor", "link", "phase",
    "macro_export", "must_use", "automatically_derived",

    //mod-level
    "path", "link_name", "link_args", "macro_escape", "no_implicit_prelude",

    // fn-level
    "test", "bench", "should_fail", "ignore", "inline", "lang", "main", "start",
    "no_split_stack", "cold", "macro_registrar", "linkage",

    // internal attribute: bypass privacy inside items
    "!resolve_unexported",
];

fn check_crate_attrs_usage(cx: &Context, attrs: &[ast::Attribute]) {

    for attr in attrs.iter() {
        let name = attr.node.value.name();
        let mut iter = crate_attrs.iter().chain(other_attrs.iter());
        if !iter.any(|other_attr| { name.equiv(other_attr) }) {
            cx.span_lint(AttributeUsage, attr.span, "unknown crate attribute");
        }
        if name.equiv(&("link")) {
            cx.tcx.sess.span_err(attr.span,
                                 "obsolete crate `link` attribute");
            cx.tcx.sess.note("the link attribute has been superceded by the crate_id \
                             attribute, which has the format `#[crate_id = \"name#version\"]`");
        }
    }
}

fn check_attrs_usage(cx: &Context, attrs: &[ast::Attribute]) {
    // check if element has crate-level, obsolete, or any unknown attributes.

    for attr in attrs.iter() {
        let name = attr.node.value.name();
        for crate_attr in crate_attrs.iter() {
            if name.equiv(crate_attr) {
                let msg = match attr.node.style {
                    ast::AttrOuter => "crate-level attribute should be an inner attribute: \
                                       add an exclamation mark: #![foo]",
                    ast::AttrInner => "crate-level attribute should be in the root module",
                };
                cx.span_lint(AttributeUsage, attr.span, msg);
                return;
            }
        }

        for &(obs_attr, obs_alter) in obsolete_attrs.iter() {
            if name.equiv(&obs_attr) {
                cx.span_lint(AttributeUsage, attr.span,
                             format!("obsolete attribute: {:s}",
                                     obs_alter).as_slice());
                return;
            }
        }

        if !other_attrs.iter().any(|other_attr| { name.equiv(other_attr) }) {
            cx.span_lint(AttributeUsage, attr.span, "unknown attribute");
        }
    }
}

fn check_unused_attribute(cx: &Context, attrs: &[ast::Attribute]) {
    static ATTRIBUTE_WHITELIST: &'static [&'static str] = &'static [
        // FIXME: #14408 whitelist docs since rustdoc looks at them
        "doc",

        // FIXME: #14406 these are processed in trans, which happens after the
        // lint pass
        "address_insignificant",
        "cold",
        "inline",
        "link",
        "link_name",
        "link_section",
        "no_builtins",
        "no_mangle",
        "no_split_stack",
        "packed",
        "static_assert",
        "thread_local",

        // not used anywhere (!?) but apparently we want to keep them around
        "comment",
        "desc",
        "license",

        // FIXME: #14407 these are only looked at on-demand so we can't
        // guarantee they'll have already been checked
        "deprecated",
        "experimental",
        "frozen",
        "locked",
        "must_use",
        "stable",
        "unstable",
    ];
    for attr in attrs.iter() {
        for &name in ATTRIBUTE_WHITELIST.iter() {
            if attr.check_name(name) {
                break;
            }
        }

        if !attr::is_used(attr) {
            cx.span_lint(UnusedAttribute, attr.span, "unused attribute");
        }
    }
}

fn check_heap_expr(cx: &Context, e: &ast::Expr) {
    let ty = ty::expr_ty(cx.tcx, e);
    check_heap_type(cx, e.span, ty);
}

fn check_path_statement(cx: &Context, s: &ast::Stmt) {
    match s.node {
        ast::StmtSemi(expr, _) => {
            match expr.node {
                ast::ExprPath(_) => {
                    cx.span_lint(PathStatement,
                                 s.span,
                                 "path statement with no effect");
                }
                _ => {}
            }
        }
        _ => ()
    }
}

fn check_unused_result(cx: &Context, s: &ast::Stmt) {
    let expr = match s.node {
        ast::StmtSemi(expr, _) => expr,
        _ => return
    };
    let t = ty::expr_ty(cx.tcx, expr);
    match ty::get(t).sty {
        ty::ty_nil | ty::ty_bot | ty::ty_bool => return,
        _ => {}
    }
    match expr.node {
        ast::ExprRet(..) => return,
        _ => {}
    }

    let t = ty::expr_ty(cx.tcx, expr);
    let mut warned = false;
    match ty::get(t).sty {
        ty::ty_struct(did, _) |
        ty::ty_enum(did, _) => {
            if ast_util::is_local(did) {
                match cx.tcx.map.get(did.node) {
                    ast_map::NodeItem(it) => {
                        if attr::contains_name(it.attrs.as_slice(),
                                               "must_use") {
                            cx.span_lint(UnusedMustUse, s.span,
                                         "unused result which must be used");
                            warned = true;
                        }
                    }
                    _ => {}
                }
            } else {
                csearch::get_item_attrs(&cx.tcx.sess.cstore, did, |attrs| {
                    if attr::contains_name(attrs.as_slice(), "must_use") {
                        cx.span_lint(UnusedMustUse, s.span,
                                     "unused result which must be used");
                        warned = true;
                    }
                });
            }
        }
        _ => {}
    }
    if !warned {
        cx.span_lint(UnusedResult, s.span, "unused result");
    }
}

fn check_deprecated_owned_vector(cx: &Context, e: &ast::Expr) {
    let t = ty::expr_ty(cx.tcx, e);
    match ty::get(t).sty {
        ty::ty_uniq(t) => match ty::get(t).sty {
            ty::ty_vec(_, None) => {
                cx.span_lint(DeprecatedOwnedVector, e.span,
                             "use of deprecated `~[]` vector; replaced by `std::vec::Vec`")
            }
            _ => {}
        },
        _ => {}
    }
}

fn check_item_non_camel_case_types(cx: &Context, it: &ast::Item) {
    fn is_camel_case(ident: ast::Ident) -> bool {
        let ident = token::get_ident(ident);
        assert!(!ident.get().is_empty());
        let ident = ident.get().trim_chars('_');

        // start with a non-lowercase letter rather than non-uppercase
        // ones (some scripts don't have a concept of upper/lowercase)
        !ident.char_at(0).is_lowercase() && !ident.contains_char('_')
    }

    fn check_case(cx: &Context, sort: &str, ident: ast::Ident, span: Span) {
        if !is_camel_case(ident) {
            cx.span_lint(
                NonCamelCaseTypes, span,
                format!("{} `{}` should have a camel case identifier",
                    sort, token::get_ident(ident)).as_slice());
        }
    }

    match it.node {
        ast::ItemTy(..) | ast::ItemStruct(..) => {
            check_case(cx, "type", it.ident, it.span)
        }
        ast::ItemTrait(..) => {
            check_case(cx, "trait", it.ident, it.span)
        }
        ast::ItemEnum(ref enum_definition, _) => {
            check_case(cx, "type", it.ident, it.span);
            for variant in enum_definition.variants.iter() {
                check_case(cx, "variant", variant.node.name, variant.span);
            }
        }
        _ => ()
    }
}

fn check_item_non_uppercase_statics(cx: &Context, it: &ast::Item) {
    match it.node {
        // only check static constants
        ast::ItemStatic(_, ast::MutImmutable, _) => {
            let s = token::get_ident(it.ident);
            // check for lowercase letters rather than non-uppercase
            // ones (some scripts don't have a concept of
            // upper/lowercase)
            if s.get().chars().any(|c| c.is_lowercase()) {
                cx.span_lint(NonUppercaseStatics, it.span,
                             "static constant should have an uppercase identifier");
            }
        }
        _ => {}
    }
}

fn check_pat_non_uppercase_statics(cx: &Context, p: &ast::Pat) {
    // Lint for constants that look like binding identifiers (#7526)
    match (&p.node, cx.tcx.def_map.borrow().find(&p.id)) {
        (&ast::PatIdent(_, ref path, _), Some(&ast::DefStatic(_, false))) => {
            // last identifier alone is right choice for this lint.
            let ident = path.segments.last().unwrap().identifier;
            let s = token::get_ident(ident);
            if s.get().chars().any(|c| c.is_lowercase()) {
                cx.span_lint(NonUppercasePatternStatics, path.span,
                             "static constant in pattern should be all caps");
            }
        }
        _ => {}
    }
}

fn check_pat_uppercase_variable(cx: &Context, p: &ast::Pat) {
    match &p.node {
        &ast::PatIdent(_, ref path, _) => {
            match cx.tcx.def_map.borrow().find(&p.id) {
                Some(&ast::DefLocal(_, _)) | Some(&ast::DefBinding(_, _)) |
                        Some(&ast::DefArg(_, _)) => {
                    // last identifier alone is right choice for this lint.
                    let ident = path.segments.last().unwrap().identifier;
                    let s = token::get_ident(ident);
                    if s.get().len() > 0 && s.get().char_at(0).is_uppercase() {
                        cx.span_lint(
                            UppercaseVariables,
                            path.span,
                            "variable names should start with a lowercase character");
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
}

fn check_struct_uppercase_variable(cx: &Context, s: &ast::StructDef) {
    for sf in s.fields.iter() {
        match sf.node {
            ast::StructField_ { kind: ast::NamedField(ident, _), .. } => {
                let s = token::get_ident(ident);
                if s.get().char_at(0).is_uppercase() {
                    cx.span_lint(
                        UppercaseVariables,
                        sf.span,
                        "structure field names should start with a lowercase character");
                }
            }
            _ => {}
        }
    }
}

fn check_unnecessary_parens_core(cx: &Context, value: &ast::Expr, msg: &str) {
    match value.node {
        ast::ExprParen(_) => {
            cx.span_lint(UnnecessaryParens, value.span,
                         format!("unnecessary parentheses around {}",
                                 msg).as_slice())
        }
        _ => {}
    }
}

fn check_unnecessary_parens_expr(cx: &Context, e: &ast::Expr) {
    let (value, msg) = match e.node {
        ast::ExprIf(cond, _, _) => (cond, "`if` condition"),
        ast::ExprWhile(cond, _) => (cond, "`while` condition"),
        ast::ExprMatch(head, _) => (head, "`match` head expression"),
        ast::ExprRet(Some(value)) => (value, "`return` value"),
        ast::ExprAssign(_, value) => (value, "assigned value"),
        ast::ExprAssignOp(_, _, value) => (value, "assigned value"),
        _ => return
    };
    check_unnecessary_parens_core(cx, value, msg);
}

fn check_unnecessary_parens_stmt(cx: &Context, s: &ast::Stmt) {
    let (value, msg) = match s.node {
        ast::StmtDecl(decl, _) => match decl.node {
            ast::DeclLocal(local) => match local.init {
                Some(value) => (value, "assigned value"),
                None => return
            },
            _ => return
        },
        _ => return
    };
    check_unnecessary_parens_core(cx, value, msg);
}

fn check_unused_unsafe(cx: &Context, e: &ast::Expr) {
    match e.node {
        // Don't warn about generated blocks, that'll just pollute the output.
        ast::ExprBlock(ref blk) => {
            if blk.rules == ast::UnsafeBlock(ast::UserProvided) &&
                !cx.tcx.used_unsafe.borrow().contains(&blk.id) {
                cx.span_lint(UnusedUnsafe, blk.span,
                             "unnecessary `unsafe` block");
            }
        }
        _ => ()
    }
}

fn check_unsafe_block(cx: &Context, e: &ast::Expr) {
    match e.node {
        // Don't warn about generated blocks, that'll just pollute the output.
        ast::ExprBlock(ref blk) if blk.rules == ast::UnsafeBlock(ast::UserProvided) => {
            cx.span_lint(UnsafeBlock, blk.span, "usage of an `unsafe` block");
        }
        _ => ()
    }
}

fn check_unused_mut_pat(cx: &Context, pats: &[@ast::Pat]) {
    // collect all mutable pattern and group their NodeIDs by their Identifier to
    // avoid false warnings in match arms with multiple patterns
    let mut mutables = HashMap::new();
    for &p in pats.iter() {
        pat_util::pat_bindings(&cx.tcx.def_map, p, |mode, id, _, path| {
            match mode {
                ast::BindByValue(ast::MutMutable) => {
                    if path.segments.len() != 1 {
                        cx.tcx.sess.span_bug(p.span,
                                             "mutable binding that doesn't consist \
                                              of exactly one segment");
                    }
                    let ident = path.segments.get(0).identifier;
                    if !token::get_ident(ident).get().starts_with("_") {
                        mutables.insert_or_update_with(ident.name as uint, vec!(id), |_, old| {
                            old.push(id);
                        });
                    }
                }
                _ => {
                }
            }
        });
    }

    let used_mutables = cx.tcx.used_mut_nodes.borrow();
    for (_, v) in mutables.iter() {
        if !v.iter().any(|e| used_mutables.contains(e)) {
            cx.span_lint(UnusedMut, cx.tcx.map.span(*v.get(0)),
                         "variable does not need to be mutable");
        }
    }
}

enum Allocation {
    VectorAllocation,
    BoxAllocation
}

fn check_unnecessary_allocation(cx: &Context, e: &ast::Expr) {
    // Warn if string and vector literals with sigils, or boxing expressions,
    // are immediately borrowed.
    let allocation = match e.node {
        ast::ExprVstore(e2, ast::ExprVstoreUniq) => {
            match e2.node {
                ast::ExprLit(lit) if ast_util::lit_is_str(lit) => {
                    VectorAllocation
                }
                ast::ExprVec(..) => VectorAllocation,
                _ => return
            }
        }
        ast::ExprUnary(ast::UnUniq, _) |
        ast::ExprUnary(ast::UnBox, _) => BoxAllocation,

        _ => return
    };

    let report = |msg| {
        cx.span_lint(UnnecessaryAllocation, e.span, msg);
    };

    match cx.tcx.adjustments.borrow().find(&e.id) {
        Some(adjustment) => {
            match *adjustment {
                ty::AutoDerefRef(ty::AutoDerefRef { autoref, .. }) => {
                    match (allocation, autoref) {
                        (VectorAllocation, Some(ty::AutoBorrowVec(..))) => {
                            report("unnecessary allocation, the sigil can be \
                                    removed");
                        }
                        (BoxAllocation,
                         Some(ty::AutoPtr(_, ast::MutImmutable))) => {
                            report("unnecessary allocation, use & instead");
                        }
                        (BoxAllocation,
                         Some(ty::AutoPtr(_, ast::MutMutable))) => {
                            report("unnecessary allocation, use &mut \
                                    instead");
                        }
                        _ => ()
                    }
                }
                _ => {}
            }
        }

        _ => ()
    }
}

fn check_missing_doc_attrs(cx: &Context,
                           id: Option<ast::NodeId>,
                           attrs: &[ast::Attribute],
                           sp: Span,
                           desc: &'static str) {
    // If we're building a test harness, then warning about
    // documentation is probably not really relevant right now.
    if cx.tcx.sess.opts.test { return }

    // `#[doc(hidden)]` disables missing_doc check.
    if cx.is_doc_hidden { return }

    // Only check publicly-visible items, using the result from the privacy pass. It's an option so
    // the crate root can also use this function (it doesn't have a NodeId).
    match id {
        Some(ref id) if !cx.exported_items.contains(id) => return,
        _ => ()
    }

    let has_doc = attrs.iter().any(|a| {
        match a.node.value.node {
            ast::MetaNameValue(ref name, _) if name.equiv(&("doc")) => true,
            _ => false
        }
    });
    if !has_doc {
        cx.span_lint(MissingDoc,
                     sp,
                     format!("missing documentation for {}",
                             desc).as_slice());
    }
}

fn check_missing_doc_item(cx: &Context, it: &ast::Item) {
    let desc = match it.node {
        ast::ItemFn(..) => "a function",
        ast::ItemMod(..) => "a module",
        ast::ItemEnum(..) => "an enum",
        ast::ItemStruct(..) => "a struct",
        ast::ItemTrait(..) => "a trait",
        _ => return
    };
    check_missing_doc_attrs(cx,
                            Some(it.id),
                            it.attrs.as_slice(),
                            it.span,
                            desc);
}

fn check_missing_doc_method(cx: &Context, m: &ast::Method) {
    let did = ast::DefId {
        krate: ast::LOCAL_CRATE,
        node: m.id
    };

    match cx.tcx.methods.borrow().find_copy(&did) {
        None => cx.tcx.sess.span_bug(m.span, "missing method descriptor?!"),
        Some(md) => {
            match md.container {
                // Always check default methods defined on traits.
                ty::TraitContainer(..) => {}
                // For methods defined on impls, it depends on whether
                // it is an implementation for a trait or is a plain
                // impl.
                ty::ImplContainer(cid) => {
                    match ty::impl_trait_ref(cx.tcx, cid) {
                        Some(..) => return, // impl for trait: don't doc
                        None => {} // plain impl: doc according to privacy
                    }
                }
            }
        }
    }
    check_missing_doc_attrs(cx,
                            Some(m.id),
                            m.attrs.as_slice(),
                            m.span,
                            "a method");
}

fn check_missing_doc_ty_method(cx: &Context, tm: &ast::TypeMethod) {
    check_missing_doc_attrs(cx,
                            Some(tm.id),
                            tm.attrs.as_slice(),
                            tm.span,
                            "a type method");
}

fn check_missing_doc_struct_field(cx: &Context, sf: &ast::StructField) {
    match sf.node.kind {
        ast::NamedField(_, vis) if vis == ast::Public =>
            check_missing_doc_attrs(cx,
                                    Some(cx.cur_struct_def_id),
                                    sf.node.attrs.as_slice(),
                                    sf.span,
                                    "a struct field"),
        _ => {}
    }
}

fn check_missing_doc_variant(cx: &Context, v: &ast::Variant) {
    check_missing_doc_attrs(cx,
                            Some(v.node.id),
                            v.node.attrs.as_slice(),
                            v.span,
                            "a variant");
}

/// Checks for use of items with #[deprecated], #[experimental] and
/// #[unstable] (or none of them) attributes.
fn check_stability(cx: &Context, e: &ast::Expr) {
    let id = match e.node {
        ast::ExprPath(..) | ast::ExprStruct(..) => {
            match cx.tcx.def_map.borrow().find(&e.id) {
                Some(&def) => ast_util::def_id_of_def(def),
                None => return
            }
        }
        ast::ExprMethodCall(..) => {
            let method_call = typeck::MethodCall::expr(e.id);
            match cx.tcx.method_map.borrow().find(&method_call) {
                Some(method) => {
                    match method.origin {
                        typeck::MethodStatic(def_id) => {
                            // If this implements a trait method, get def_id
                            // of the method inside trait definition.
                            // Otherwise, use the current def_id (which refers
                            // to the method inside impl).
                            ty::trait_method_of_method(
                                cx.tcx, def_id).unwrap_or(def_id)
                        }
                        typeck::MethodParam(typeck::MethodParam {
                            trait_id: trait_id,
                            method_num: index,
                            ..
                        })
                        | typeck::MethodObject(typeck::MethodObject {
                            trait_id: trait_id,
                            method_num: index,
                            ..
                        }) => ty::trait_method(cx.tcx, trait_id, index).def_id
                    }
                }
                None => return
            }
        }
        _ => return
    };

    let stability = if ast_util::is_local(id) {
        // this crate
        let s = cx.tcx.map.with_attrs(id.node, |attrs| {
            attrs.map(|a| attr::find_stability(a.as_slice()))
        });
        match s {
            Some(s) => s,

            // no possibility of having attributes
            // (e.g. it's a local variable), so just
            // ignore it.
            None => return
        }
    } else {
        // cross-crate

        let mut s = None;
        // run through all the attributes and take the first
        // stability one.
        csearch::get_item_attrs(&cx.tcx.sess.cstore, id, |attrs| {
            if s.is_none() {
                s = attr::find_stability(attrs.as_slice())
            }
        });
        s
    };

    let (lint, label) = match stability {
        // no stability attributes == Unstable
        None => (Unstable, "unmarked"),
        Some(attr::Stability { level: attr::Unstable, .. }) =>
                (Unstable, "unstable"),
        Some(attr::Stability { level: attr::Experimental, .. }) =>
                (Experimental, "experimental"),
        Some(attr::Stability { level: attr::Deprecated, .. }) =>
                (Deprecated, "deprecated"),
        _ => return
    };

    let msg = match stability {
        Some(attr::Stability { text: Some(ref s), .. }) => {
            format!("use of {} item: {}", label, *s)
        }
        _ => format!("use of {} item", label)
    };

    cx.span_lint(lint, e.span, msg.as_slice());
}

fn check_enum_variant_sizes(cx: &mut Context, it: &ast::Item) {
    match it.node {
        ast::ItemEnum(..) => {
            match cx.cur.find(&(VariantSizeDifference as uint)) {
                Some(&(lvl, src)) if lvl != Allow => {
                    cx.node_levels.insert((it.id, VariantSizeDifference), (lvl, src));
                },
                _ => { }
            }
        },
        _ => { }
    }
}

impl<'a> Visitor<()> for Context<'a> {
    fn visit_item(&mut self, it: &ast::Item, _: ()) {
        self.with_lint_attrs(it.attrs.as_slice(), |cx| {
            check_enum_variant_sizes(cx, it);
            check_item_ctypes(cx, it);
            check_item_non_camel_case_types(cx, it);
            check_item_non_uppercase_statics(cx, it);
            check_heap_item(cx, it);
            check_missing_doc_item(cx, it);
            check_attrs_usage(cx, it.attrs.as_slice());
            check_unused_attribute(cx, it.attrs.as_slice());
            check_raw_ptr_deriving(cx, it);

            cx.visit_ids(|v| v.visit_item(it, ()));

            visit::walk_item(cx, it, ());
        })
    }

    fn visit_foreign_item(&mut self, it: &ast::ForeignItem, _: ()) {
        self.with_lint_attrs(it.attrs.as_slice(), |cx| {
            check_attrs_usage(cx, it.attrs.as_slice());
            visit::walk_foreign_item(cx, it, ());
        })
    }

    fn visit_view_item(&mut self, i: &ast::ViewItem, _: ()) {
        self.with_lint_attrs(i.attrs.as_slice(), |cx| {
            check_attrs_usage(cx, i.attrs.as_slice());

            cx.visit_ids(|v| v.visit_view_item(i, ()));

            visit::walk_view_item(cx, i, ());
        })
    }

    fn visit_pat(&mut self, p: &ast::Pat, _: ()) {
        check_pat_non_uppercase_statics(self, p);
        check_pat_uppercase_variable(self, p);

        visit::walk_pat(self, p, ());
    }

    fn visit_expr(&mut self, e: &ast::Expr, _: ()) {
        match e.node {
            ast::ExprUnary(ast::UnNeg, expr) => {
                // propagate negation, if the negation itself isn't negated
                if self.negated_expr_id != e.id {
                    self.negated_expr_id = expr.id;
                }
            },
            ast::ExprParen(expr) => if self.negated_expr_id == e.id {
                self.negated_expr_id = expr.id
            },
            ast::ExprMatch(_, ref arms) => {
                for a in arms.iter() {
                    check_unused_mut_pat(self, a.pats.as_slice());
                }
            },
            _ => ()
        };

        check_while_true_expr(self, e);
        check_stability(self, e);
        check_unnecessary_parens_expr(self, e);
        check_unused_unsafe(self, e);
        check_unsafe_block(self, e);
        check_unnecessary_allocation(self, e);
        check_heap_expr(self, e);

        check_type_limits(self, e);
        check_unused_casts(self, e);
        check_deprecated_owned_vector(self, e);

        visit::walk_expr(self, e, ());
    }

    fn visit_stmt(&mut self, s: &ast::Stmt, _: ()) {
        check_path_statement(self, s);
        check_unused_result(self, s);
        check_unnecessary_parens_stmt(self, s);

        match s.node {
            ast::StmtDecl(d, _) => {
                match d.node {
                    ast::DeclLocal(l) => {
                        check_unused_mut_pat(self, &[l.pat]);
                    },
                    _ => {}
                }
            },
            _ => {}
        }

        visit::walk_stmt(self, s, ());
    }

    fn visit_fn(&mut self, fk: &visit::FnKind, decl: &ast::FnDecl,
                body: &ast::Block, span: Span, id: ast::NodeId, _: ()) {
        let recurse = |this: &mut Context| {
            visit::walk_fn(this, fk, decl, body, span, ());
        };

        for a in decl.inputs.iter(){
            check_unused_mut_pat(self, &[a.pat]);
        }

        match *fk {
            visit::FkMethod(_, _, m) => {
                self.with_lint_attrs(m.attrs.as_slice(), |cx| {
                    check_missing_doc_method(cx, m);
                    check_attrs_usage(cx, m.attrs.as_slice());

                    cx.visit_ids(|v| {
                        v.visit_fn(fk, decl, body, span, id, ());
                    });
                    recurse(cx);
                })
            }
            _ => recurse(self),
        }
    }


    fn visit_ty_method(&mut self, t: &ast::TypeMethod, _: ()) {
        self.with_lint_attrs(t.attrs.as_slice(), |cx| {
            check_missing_doc_ty_method(cx, t);
            check_attrs_usage(cx, t.attrs.as_slice());

            visit::walk_ty_method(cx, t, ());
        })
    }

    fn visit_struct_def(&mut self,
                        s: &ast::StructDef,
                        _: ast::Ident,
                        _: &ast::Generics,
                        id: ast::NodeId,
                        _: ()) {
        check_struct_uppercase_variable(self, s);

        let old_id = self.cur_struct_def_id;
        self.cur_struct_def_id = id;
        visit::walk_struct_def(self, s, ());
        self.cur_struct_def_id = old_id;
    }

    fn visit_struct_field(&mut self, s: &ast::StructField, _: ()) {
        self.with_lint_attrs(s.node.attrs.as_slice(), |cx| {
            check_missing_doc_struct_field(cx, s);
            check_attrs_usage(cx, s.node.attrs.as_slice());

            visit::walk_struct_field(cx, s, ());
        })
    }

    fn visit_variant(&mut self, v: &ast::Variant, g: &ast::Generics, _: ()) {
        self.with_lint_attrs(v.node.attrs.as_slice(), |cx| {
            check_missing_doc_variant(cx, v);
            check_attrs_usage(cx, v.node.attrs.as_slice());

            visit::walk_variant(cx, v, g, ());
        })
    }

    // FIXME(#10894) should continue recursing
    fn visit_ty(&mut self, _t: &ast::Ty, _: ()) {}
}

impl<'a> IdVisitingOperation for Context<'a> {
    fn visit_id(&self, id: ast::NodeId) {
        match self.tcx.sess.lints.borrow_mut().pop(&id) {
            None => {}
            Some(l) => {
                for (lint, span, msg) in l.move_iter() {
                    self.span_lint(lint, span, msg.as_slice())
                }
            }
        }
    }
}

pub fn check_crate(tcx: &ty::ctxt,
                   exported_items: &privacy::ExportedItems,
                   krate: &ast::Crate) {
    let mut cx = Context {
        dict: get_lint_dict(),
        cur: SmallIntMap::new(),
        tcx: tcx,
        exported_items: exported_items,
        cur_struct_def_id: -1,
        is_doc_hidden: false,
        lint_stack: Vec::new(),
        negated_expr_id: -1,
        checked_raw_pointers: NodeSet::new(),
        node_levels: HashMap::new(),
    };

    // Install default lint levels, followed by the command line levels, and
    // then actually visit the whole crate.
    for (_, spec) in cx.dict.iter() {
        if spec.default != Allow {
            cx.cur.insert(spec.lint as uint, (spec.default, Default));
        }
    }
    for &(lint, level) in tcx.sess.opts.lint_opts.iter() {
        cx.set_level(lint, level, CommandLine);
    }
    cx.with_lint_attrs(krate.attrs.as_slice(), |cx| {
        cx.visit_id(ast::CRATE_NODE_ID);
        cx.visit_ids(|v| {
            v.visited_outermost = true;
            visit::walk_crate(v, krate, ());
        });

        check_crate_attrs_usage(cx, krate.attrs.as_slice());
        // since the root module isn't visited as an item (because it isn't an item), warn for it
        // here.
        check_unused_attribute(cx, krate.attrs.as_slice());
        check_missing_doc_attrs(cx,
                                None,
                                krate.attrs.as_slice(),
                                krate.span,
                                "crate");

        visit::walk_crate(cx, krate, ());
    });

    // If we missed any lints added to the session, then there's a bug somewhere
    // in the iteration code.
    for (id, v) in tcx.sess.lints.borrow().iter() {
        for &(lint, span, ref msg) in v.iter() {
            tcx.sess.span_bug(span, format!("unprocessed lint {} at {}: {}",
                                            lint, tcx.map.node_to_str(*id), *msg).as_slice())
        }
    }

    tcx.sess.abort_if_errors();
    *tcx.node_lint_levels.borrow_mut() = cx.node_levels;
}
