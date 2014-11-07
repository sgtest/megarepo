// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Lints built in to rustc.
//!
//! This is a sibling of `lint::context` in order to ensure that
//! lints implemented here use the same public API as lint plugins.
//!
//! To add a new lint to rustc, declare it here using `declare_lint!()`.
//! Then add code to emit the new lint in the appropriate circumstances.
//! You can do that in an existing `LintPass` if it makes sense, or in
//! a new `LintPass`, or using `Session::add_lint` elsewhere in the
//! compiler. Only do the latter if the check can't be written cleanly
//! as a `LintPass`.
//!
//! If you define a new `LintPass`, you will also need to add it to the
//! `add_builtin!` or `add_builtin_with_new!` invocation in `context.rs`.
//! Use the former for unit-like structs and the latter for structs with
//! a `pub fn new()`.

use metadata::csearch;
use middle::def::*;
use middle::typeck::astconv::ast_ty_to_ty;
use middle::typeck::infer;
use middle::{typeck, ty, def, pat_util, stability};
use middle::const_eval::{eval_const_expr_partial, const_int, const_uint};
use util::ppaux::{ty_to_string};
use util::nodemap::NodeSet;
use lint::{Context, LintPass, LintArray};

use std::cmp;
use std::collections::HashMap;
use std::collections::hash_map::{Occupied, Vacant};
use std::slice;
use std::{i8, i16, i32, i64, u8, u16, u32, u64, f32, f64};
use syntax::abi;
use syntax::ast_map;
use syntax::ast_util::is_shift_binop;
use syntax::attr::AttrMetaMethods;
use syntax::attr;
use syntax::codemap::{Span, DUMMY_SP};
use syntax::parse::token;
use syntax::{ast, ast_util, visit};
use syntax::ast::{TyI, TyU, TyI8, TyU8, TyI16, TyU16, TyI32, TyU32, TyI64, TyU64};
use syntax::ptr::P;
use syntax::visit::Visitor;

declare_lint!(WHILE_TRUE, Warn,
              "suggest using `loop { }` instead of `while true { }`")

pub struct WhileTrue;

impl LintPass for WhileTrue {
    fn get_lints(&self) -> LintArray {
        lint_array!(WHILE_TRUE)
    }

    fn check_expr(&mut self, cx: &Context, e: &ast::Expr) {
        match e.node {
            ast::ExprWhile(ref cond, _, _) => {
                match cond.node {
                    ast::ExprLit(ref lit) => {
                        match lit.node {
                            ast::LitBool(true) => {
                                cx.span_lint(WHILE_TRUE, e.span,
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
}

declare_lint!(UNUSED_TYPECASTS, Allow,
              "detects unnecessary type casts, that can be removed")

pub struct UnusedCasts;

impl LintPass for UnusedCasts {
    fn get_lints(&self) -> LintArray {
        lint_array!(UNUSED_TYPECASTS)
    }

    fn check_expr(&mut self, cx: &Context, e: &ast::Expr) {
        match e.node {
            ast::ExprCast(ref expr, ref ty) => {
                let t_t = ast_ty_to_ty(cx, &infer::new_infer_ctxt(cx.tcx), &**ty);
                if ty::get(ty::expr_ty(cx.tcx, &**expr)).sty == ty::get(t_t).sty {
                    cx.span_lint(UNUSED_TYPECASTS, ty.span, "unnecessary type cast");
                }
            }
            _ => ()
        }
    }
}

declare_lint!(UNSIGNED_NEGATION, Warn,
              "using an unary minus operator on unsigned type")

declare_lint!(UNUSED_COMPARISONS, Warn,
              "comparisons made useless by limits of the types involved")

declare_lint!(OVERFLOWING_LITERALS, Warn,
              "literal out of range for its type")

declare_lint!(EXCEEDING_BITSHIFTS, Allow,
              "shift exceeds the type's number of bits")

pub struct TypeLimits {
    /// Id of the last visited negated expression
    negated_expr_id: ast::NodeId,
}

impl TypeLimits {
    pub fn new() -> TypeLimits {
        TypeLimits {
            negated_expr_id: -1,
        }
    }
}

impl LintPass for TypeLimits {
    fn get_lints(&self) -> LintArray {
        lint_array!(UNSIGNED_NEGATION, UNUSED_COMPARISONS, OVERFLOWING_LITERALS,
                    EXCEEDING_BITSHIFTS)
    }

    fn check_expr(&mut self, cx: &Context, e: &ast::Expr) {
        match e.node {
            ast::ExprUnary(ast::UnNeg, ref expr) => {
                match expr.node  {
                    ast::ExprLit(ref lit) => {
                        match lit.node {
                            ast::LitInt(_, ast::UnsignedIntLit(_)) => {
                                cx.span_lint(UNSIGNED_NEGATION, e.span,
                                             "negation of unsigned int literal may \
                                             be unintentional");
                            },
                            _ => ()
                        }
                    },
                    _ => {
                        let t = ty::expr_ty(cx.tcx, &**expr);
                        match ty::get(t).sty {
                            ty::ty_uint(_) => {
                                cx.span_lint(UNSIGNED_NEGATION, e.span,
                                             "negation of unsigned int variable may \
                                             be unintentional");
                            },
                            _ => ()
                        }
                    }
                };
                // propagate negation, if the negation itself isn't negated
                if self.negated_expr_id != e.id {
                    self.negated_expr_id = expr.id;
                }
            },
            ast::ExprParen(ref expr) if self.negated_expr_id == e.id => {
                self.negated_expr_id = expr.id;
            },
            ast::ExprBinary(binop, ref l, ref r) => {
                if is_comparison(binop) && !check_limits(cx.tcx, binop, &**l, &**r) {
                    cx.span_lint(UNUSED_COMPARISONS, e.span,
                                 "comparison is useless due to type limits");
                }

                if is_shift_binop(binop) {
                    let opt_ty_bits = match ty::get(ty::expr_ty(cx.tcx, &**l)).sty {
                        ty::ty_int(t) => Some(int_ty_bits(t, cx.sess().target.int_type)),
                        ty::ty_uint(t) => Some(uint_ty_bits(t, cx.sess().target.uint_type)),
                        _ => None
                    };

                    if let Some(bits) = opt_ty_bits {
                        let exceeding = if let ast::ExprLit(ref lit) = r.node {
                            if let ast::LitInt(shift, _) = lit.node { shift >= bits }
                            else { false }
                        } else {
                            match eval_const_expr_partial(cx.tcx, &**r) {
                                Ok(const_int(shift)) => { shift as u64 >= bits },
                                Ok(const_uint(shift)) => { shift >= bits },
                                _ => { false }
                            }
                        };
                        if exceeding {
                            cx.span_lint(EXCEEDING_BITSHIFTS, e.span,
                                         "bitshift exceeds the type's number of bits");
                        }
                    };
                }
            },
            ast::ExprLit(ref lit) => {
                match ty::get(ty::expr_ty(cx.tcx, e)).sty {
                    ty::ty_int(t) => {
                        match lit.node {
                            ast::LitInt(v, ast::SignedIntLit(_, ast::Plus)) |
                            ast::LitInt(v, ast::UnsuffixedIntLit(ast::Plus)) => {
                                let int_type = if t == ast::TyI {
                                    cx.sess().target.int_type
                                } else { t };
                                let (min, max) = int_ty_range(int_type);
                                let negative = self.negated_expr_id == e.id;

                                if (negative && v > (min.abs() as u64)) ||
                                   (!negative && v > (max.abs() as u64)) {
                                    cx.span_lint(OVERFLOWING_LITERALS, e.span,
                                                 "literal out of range for its type");
                                    return;
                                }
                            }
                            _ => panic!()
                        };
                    },
                    ty::ty_uint(t) => {
                        let uint_type = if t == ast::TyU {
                            cx.sess().target.uint_type
                        } else { t };
                        let (min, max) = uint_ty_range(uint_type);
                        let lit_val: u64 = match lit.node {
                            ast::LitByte(_v) => return,  // _v is u8, within range by definition
                            ast::LitInt(v, _) => v,
                            _ => panic!()
                        };
                        if  lit_val < min || lit_val > max {
                            cx.span_lint(OVERFLOWING_LITERALS, e.span,
                                         "literal out of range for its type");
                        }
                    },
                    ty::ty_float(t) => {
                        let (min, max) = float_ty_range(t);
                        let lit_val: f64 = match lit.node {
                            ast::LitFloat(ref v, _) |
                            ast::LitFloatUnsuffixed(ref v) => match from_str(v.get()) {
                                Some(f) => f,
                                None => return
                            },
                            _ => panic!()
                        };
                        if lit_val < min || lit_val > max {
                            cx.span_lint(OVERFLOWING_LITERALS, e.span,
                                         "literal out of range for its type");
                        }
                    },
                    _ => ()
                };
            },
            _ => ()
        };

        fn is_valid<T:cmp::PartialOrd>(binop: ast::BinOp, v: T,
                                min: T, max: T) -> bool {
            match binop {
                ast::BiLt => v >  min && v <= max,
                ast::BiLe => v >= min && v <  max,
                ast::BiGt => v >= min && v <  max,
                ast::BiGe => v >  min && v <= max,
                ast::BiEq | ast::BiNe => v >= min && v <= max,
                _ => panic!()
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

        fn float_ty_range(float_ty: ast::FloatTy) -> (f64, f64) {
            match float_ty {
                ast::TyF32  => (f32::MIN_VALUE as f64, f32::MAX_VALUE as f64),
                ast::TyF64  => (f64::MIN_VALUE,        f64::MAX_VALUE)
            }
        }

        fn int_ty_bits(int_ty: ast::IntTy, target_int_ty: ast::IntTy) -> u64 {
            match int_ty {
                ast::TyI =>    int_ty_bits(target_int_ty, target_int_ty),
                ast::TyI8 =>   i8::BITS  as u64,
                ast::TyI16 =>  i16::BITS as u64,
                ast::TyI32 =>  i32::BITS as u64,
                ast::TyI64 =>  i64::BITS as u64
            }
        }

        fn uint_ty_bits(uint_ty: ast::UintTy, target_uint_ty: ast::UintTy) -> u64 {
            match uint_ty {
                ast::TyU =>    uint_ty_bits(target_uint_ty, target_uint_ty),
                ast::TyU8 =>   u8::BITS  as u64,
                ast::TyU16 =>  u16::BITS as u64,
                ast::TyU32 =>  u32::BITS as u64,
                ast::TyU64 =>  u64::BITS as u64
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
                        ast::ExprLit(ref li) => match li.node {
                            ast::LitInt(v, ast::SignedIntLit(_, ast::Plus)) |
                            ast::LitInt(v, ast::UnsuffixedIntLit(ast::Plus)) => v as i64,
                            ast::LitInt(v, ast::SignedIntLit(_, ast::Minus)) |
                            ast::LitInt(v, ast::UnsuffixedIntLit(ast::Minus)) => -(v as i64),
                            _ => return true
                        },
                        _ => panic!()
                    };
                    is_valid(norm_binop, lit_val, min, max)
                }
                ty::ty_uint(uint_ty) => {
                    let (min, max): (u64, u64) = uint_ty_range(uint_ty);
                    let lit_val: u64 = match lit.node {
                        ast::ExprLit(ref li) => match li.node {
                            ast::LitInt(v, _) => v,
                            _ => return true
                        },
                        _ => panic!()
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
}

declare_lint!(IMPROPER_CTYPES, Warn,
              "proper use of libc types in foreign modules")

struct ImproperCTypesVisitor<'a, 'tcx: 'a> {
    cx: &'a Context<'a, 'tcx>
}

impl<'a, 'tcx> ImproperCTypesVisitor<'a, 'tcx> {
    fn check_def(&mut self, sp: Span, ty_id: ast::NodeId, path_id: ast::NodeId) {
        match self.cx.tcx.def_map.borrow().get_copy(&path_id) {
            def::DefPrimTy(ast::TyInt(ast::TyI)) => {
                self.cx.span_lint(IMPROPER_CTYPES, sp,
                                  "found rust type `int` in foreign module, while \
                                   libc::c_int or libc::c_long should be used");
            }
            def::DefPrimTy(ast::TyUint(ast::TyU)) => {
                self.cx.span_lint(IMPROPER_CTYPES, sp,
                                  "found rust type `uint` in foreign module, while \
                                   libc::c_uint or libc::c_ulong should be used");
            }
            def::DefTy(..) => {
                let tty = match self.cx.tcx.ast_ty_to_ty_cache.borrow().get(&ty_id) {
                    Some(&ty::atttce_resolved(t)) => t,
                    _ => panic!("ast_ty_to_ty_cache was incomplete after typeck!")
                };

                if !ty::is_ffi_safe(self.cx.tcx, tty) {
                    self.cx.span_lint(IMPROPER_CTYPES, sp,
                                      "found type without foreign-function-safe
                                      representation annotation in foreign module, consider \
                                      adding a #[repr(...)] attribute to the type");
                }
            }
            _ => ()
        }
    }
}

impl<'a, 'tcx, 'v> Visitor<'v> for ImproperCTypesVisitor<'a, 'tcx> {
    fn visit_ty(&mut self, ty: &ast::Ty) {
        match ty.node {
            ast::TyPath(_, _, id) => self.check_def(ty.span, ty.id, id),
            _ => (),
        }
        visit::walk_ty(self, ty);
    }
}

pub struct ImproperCTypes;

impl LintPass for ImproperCTypes {
    fn get_lints(&self) -> LintArray {
        lint_array!(IMPROPER_CTYPES)
    }

    fn check_item(&mut self, cx: &Context, it: &ast::Item) {
        fn check_ty(cx: &Context, ty: &ast::Ty) {
            let mut vis = ImproperCTypesVisitor { cx: cx };
            vis.visit_ty(ty);
        }

        fn check_foreign_fn(cx: &Context, decl: &ast::FnDecl) {
            for input in decl.inputs.iter() {
                check_ty(cx, &*input.ty);
            }
            check_ty(cx, &*decl.output)
        }

        match it.node {
            ast::ItemForeignMod(ref nmod) if nmod.abi != abi::RustIntrinsic => {
                for ni in nmod.items.iter() {
                    match ni.node {
                        ast::ForeignItemFn(ref decl, _) => check_foreign_fn(cx, &**decl),
                        ast::ForeignItemStatic(ref t, _) => check_ty(cx, &**t)
                    }
                }
            }
            _ => (),
        }
    }
}

declare_lint!(BOX_POINTERS, Allow,
              "use of owned (Box type) heap memory")

pub struct BoxPointers;

impl BoxPointers {
    fn check_heap_type(&self, cx: &Context, span: Span, ty: ty::t) {
        let mut n_uniq = 0i;
        ty::fold_ty(cx.tcx, ty, |t| {
            match ty::get(t).sty {
                ty::ty_uniq(_) |
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

        if n_uniq > 0 {
            let s = ty_to_string(cx.tcx, ty);
            let m = format!("type uses owned (Box type) pointers: {}", s);
            cx.span_lint(BOX_POINTERS, span, m.as_slice());
        }
    }
}

impl LintPass for BoxPointers {
    fn get_lints(&self) -> LintArray {
        lint_array!(BOX_POINTERS)
    }

    fn check_item(&mut self, cx: &Context, it: &ast::Item) {
        match it.node {
            ast::ItemFn(..) |
            ast::ItemTy(..) |
            ast::ItemEnum(..) |
            ast::ItemStruct(..) =>
                self.check_heap_type(cx, it.span,
                                     ty::node_id_to_type(cx.tcx, it.id)),
            _ => ()
        }

        // If it's a struct, we also have to check the fields' types
        match it.node {
            ast::ItemStruct(ref struct_def, _) => {
                for struct_field in struct_def.fields.iter() {
                    self.check_heap_type(cx, struct_field.span,
                                         ty::node_id_to_type(cx.tcx, struct_field.node.id));
                }
            }
            _ => ()
        }
    }

    fn check_expr(&mut self, cx: &Context, e: &ast::Expr) {
        let ty = ty::expr_ty(cx.tcx, e);
        self.check_heap_type(cx, e.span, ty);
    }
}

declare_lint!(RAW_POINTER_DERIVING, Warn,
              "uses of #[deriving] with raw pointers are rarely correct")

struct RawPtrDerivingVisitor<'a, 'tcx: 'a> {
    cx: &'a Context<'a, 'tcx>
}

impl<'a, 'tcx, 'v> Visitor<'v> for RawPtrDerivingVisitor<'a, 'tcx> {
    fn visit_ty(&mut self, ty: &ast::Ty) {
        static MSG: &'static str = "use of `#[deriving]` with a raw pointer";
        match ty.node {
            ast::TyPtr(..) => self.cx.span_lint(RAW_POINTER_DERIVING, ty.span, MSG),
            _ => {}
        }
        visit::walk_ty(self, ty);
    }
    // explicit override to a no-op to reduce code bloat
    fn visit_expr(&mut self, _: &ast::Expr) {}
    fn visit_block(&mut self, _: &ast::Block) {}
}

pub struct RawPointerDeriving {
    checked_raw_pointers: NodeSet,
}

impl RawPointerDeriving {
    pub fn new() -> RawPointerDeriving {
        RawPointerDeriving {
            checked_raw_pointers: NodeSet::new(),
        }
    }
}

impl LintPass for RawPointerDeriving {
    fn get_lints(&self) -> LintArray {
        lint_array!(RAW_POINTER_DERIVING)
    }

    fn check_item(&mut self, cx: &Context, item: &ast::Item) {
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
        if !self.checked_raw_pointers.insert(item.id) { return }
        match item.node {
            ast::ItemStruct(..) | ast::ItemEnum(..) => {
                let mut visitor = RawPtrDerivingVisitor { cx: cx };
                visit::walk_item(&mut visitor, &*item);
            }
            _ => {}
        }
    }
}

declare_lint!(UNUSED_ATTRIBUTES, Warn,
              "detects attributes that were not used by the compiler")

pub struct UnusedAttributes;

impl LintPass for UnusedAttributes {
    fn get_lints(&self) -> LintArray {
        lint_array!(UNUSED_ATTRIBUTES)
    }

    fn check_attribute(&mut self, cx: &Context, attr: &ast::Attribute) {
        static ATTRIBUTE_WHITELIST: &'static [&'static str] = &[
            // FIXME: #14408 whitelist docs since rustdoc looks at them
            "doc",

            // FIXME: #14406 these are processed in trans, which happens after the
            // lint pass
            "cold",
            "export_name",
            "inline",
            "link",
            "link_name",
            "link_section",
            "no_builtins",
            "no_mangle",
            "no_split_stack",
            "no_stack_check",
            "packed",
            "static_assert",
            "thread_local",
            "no_debug",
            "unsafe_no_drop_flag",

            // used in resolve
            "prelude_import",

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

        static CRATE_ATTRS: &'static [&'static str] = &[
            "crate_name",
            "crate_type",
            "feature",
            "no_start",
            "no_main",
            "no_std",
            "desc",
            "comment",
            "license",
            "copyright",
            "no_builtins",
        ];

        for &name in ATTRIBUTE_WHITELIST.iter() {
            if attr.check_name(name) {
                break;
            }
        }

        if !attr::is_used(attr) {
            cx.span_lint(UNUSED_ATTRIBUTES, attr.span, "unused attribute");
            if CRATE_ATTRS.contains(&attr.name().get()) {
                let msg = match attr.node.style {
                    ast::AttrOuter => "crate-level attribute should be an inner \
                                       attribute: add an exclamation mark: #![foo]",
                    ast::AttrInner => "crate-level attribute should be in the \
                                       root module",
                };
                cx.span_lint(UNUSED_ATTRIBUTES, attr.span, msg);
            }
        }
    }
}

declare_lint!(pub PATH_STATEMENTS, Warn,
              "path statements with no effect")

pub struct PathStatements;

impl LintPass for PathStatements {
    fn get_lints(&self) -> LintArray {
        lint_array!(PATH_STATEMENTS)
    }

    fn check_stmt(&mut self, cx: &Context, s: &ast::Stmt) {
        match s.node {
            ast::StmtSemi(ref expr, _) => {
                match expr.node {
                    ast::ExprPath(_) => cx.span_lint(PATH_STATEMENTS, s.span,
                                                     "path statement with no effect"),
                    _ => ()
                }
            }
            _ => ()
        }
    }
}

declare_lint!(pub UNUSED_MUST_USE, Warn,
              "unused result of a type flagged as #[must_use]")

declare_lint!(pub UNUSED_RESULTS, Allow,
              "unused result of an expression in a statement")

pub struct UnusedResults;

impl LintPass for UnusedResults {
    fn get_lints(&self) -> LintArray {
        lint_array!(UNUSED_MUST_USE, UNUSED_RESULTS)
    }

    fn check_stmt(&mut self, cx: &Context, s: &ast::Stmt) {
        let expr = match s.node {
            ast::StmtSemi(ref expr, _) => &**expr,
            _ => return
        };

        match expr.node {
            ast::ExprRet(..) => return,
            _ => {}
        }

        let t = ty::expr_ty(cx.tcx, expr);
        let mut warned = false;
        match ty::get(t).sty {
            ty::ty_nil | ty::ty_bool => return,
            ty::ty_struct(did, _) |
            ty::ty_enum(did, _) => {
                if ast_util::is_local(did) {
                    match cx.tcx.map.get(did.node) {
                        ast_map::NodeItem(it) => {
                            warned |= check_must_use(cx, it.attrs.as_slice(), s.span);
                        }
                        _ => {}
                    }
                } else {
                    csearch::get_item_attrs(&cx.sess().cstore, did, |attrs| {
                        warned |= check_must_use(cx, attrs.as_slice(), s.span);
                    });
                }
            }
            _ => {}
        }
        if !warned {
            cx.span_lint(UNUSED_RESULTS, s.span, "unused result");
        }

        fn check_must_use(cx: &Context, attrs: &[ast::Attribute], sp: Span) -> bool {
            for attr in attrs.iter() {
                if attr.check_name("must_use") {
                    let mut msg = "unused result which must be used".to_string();
                    // check for #[must_use="..."]
                    match attr.value_str() {
                        None => {}
                        Some(s) => {
                            msg.push_str(": ");
                            msg.push_str(s.get());
                        }
                    }
                    cx.span_lint(UNUSED_MUST_USE, sp, msg.as_slice());
                    return true;
                }
            }
            false
        }
    }
}

declare_lint!(pub NON_CAMEL_CASE_TYPES, Warn,
              "types, variants, traits and type parameters should have camel case names")

pub struct NonCamelCaseTypes;

impl NonCamelCaseTypes {
    fn check_case(&self, cx: &Context, sort: &str, ident: ast::Ident, span: Span) {
        fn is_camel_case(ident: ast::Ident) -> bool {
            let ident = token::get_ident(ident);
            if ident.get().is_empty() { return true; }
            let ident = ident.get().trim_chars('_');

            // start with a non-lowercase letter rather than non-uppercase
            // ones (some scripts don't have a concept of upper/lowercase)
            ident.len() > 0 && !ident.char_at(0).is_lowercase() && !ident.contains_char('_')
        }

        fn to_camel_case(s: &str) -> String {
            s.split('_').flat_map(|word| word.chars().enumerate().map(|(i, c)|
                if i == 0 { c.to_uppercase() }
                else { c }
            )).collect()
        }

        let s = token::get_ident(ident);

        if !is_camel_case(ident) {
            let c = to_camel_case(s.get());
            let m = if c.is_empty() {
                format!("{} `{}` should have a camel case name such as `CamelCase`", sort, s)
            } else {
                format!("{} `{}` should have a camel case name such as `{}`", sort, s, c)
            };
            cx.span_lint(NON_CAMEL_CASE_TYPES, span, m.as_slice());
        }
    }
}

impl LintPass for NonCamelCaseTypes {
    fn get_lints(&self) -> LintArray {
        lint_array!(NON_CAMEL_CASE_TYPES)
    }

    fn check_item(&mut self, cx: &Context, it: &ast::Item) {
        let has_extern_repr = it.attrs.iter().map(|attr| {
            attr::find_repr_attrs(cx.tcx.sess.diagnostic(), attr).iter()
                .any(|r| r == &attr::ReprExtern)
        }).any(|x| x);
        if has_extern_repr { return }

        match it.node {
            ast::ItemTy(..) | ast::ItemStruct(..) => {
                self.check_case(cx, "type", it.ident, it.span)
            }
            ast::ItemTrait(..) => {
                self.check_case(cx, "trait", it.ident, it.span)
            }
            ast::ItemEnum(ref enum_definition, _) => {
                if has_extern_repr { return }
                self.check_case(cx, "type", it.ident, it.span);
                for variant in enum_definition.variants.iter() {
                    self.check_case(cx, "variant", variant.node.name, variant.span);
                }
            }
            _ => ()
        }
    }

    fn check_generics(&mut self, cx: &Context, it: &ast::Generics) {
        for gen in it.ty_params.iter() {
            self.check_case(cx, "type parameter", gen.ident, gen.span);
        }
    }
}

#[deriving(PartialEq)]
enum MethodContext {
    TraitDefaultImpl,
    TraitImpl,
    PlainImpl
}

fn method_context(cx: &Context, m: &ast::Method) -> MethodContext {
    let did = ast::DefId {
        krate: ast::LOCAL_CRATE,
        node: m.id
    };

    match cx.tcx.impl_or_trait_items.borrow().find_copy(&did) {
        None => cx.sess().span_bug(m.span, "missing method descriptor?!"),
        Some(md) => {
            match md {
                ty::MethodTraitItem(md) => {
                    match md.container {
                        ty::TraitContainer(..) => TraitDefaultImpl,
                        ty::ImplContainer(cid) => {
                            match ty::impl_trait_ref(cx.tcx, cid) {
                                Some(..) => TraitImpl,
                                None => PlainImpl
                            }
                        }
                    }
                }
                ty::TypeTraitItem(typedef) => {
                    match typedef.container {
                        ty::TraitContainer(..) => TraitDefaultImpl,
                        ty::ImplContainer(cid) => {
                            match ty::impl_trait_ref(cx.tcx, cid) {
                                Some(..) => TraitImpl,
                                None => PlainImpl
                            }
                        }
                    }
                }
            }
        }
    }
}

declare_lint!(pub NON_SNAKE_CASE, Warn,
              "methods, functions, lifetime parameters and modules should have snake case names")

pub struct NonSnakeCase;

impl NonSnakeCase {
    fn check_snake_case(&self, cx: &Context, sort: &str, ident: ast::Ident, span: Span) {
        fn is_snake_case(ident: ast::Ident) -> bool {
            let ident = token::get_ident(ident);
            if ident.get().is_empty() { return true; }
            let ident = ident.get().trim_left_chars('\'');
            let ident = ident.trim_chars('_');

            let mut allow_underscore = true;
            ident.chars().all(|c| {
                allow_underscore = match c {
                    c if c.is_lowercase() || c.is_digit() => true,
                    '_' if allow_underscore => false,
                    _ => return false,
                };
                true
            })
        }

        fn to_snake_case(str: &str) -> String {
            let mut words = vec![];
            for s in str.split('_') {
                let mut last_upper = false;
                let mut buf = String::new();
                if s.is_empty() { continue; }
                for ch in s.chars() {
                    if !buf.is_empty() && buf.as_slice() != "'"
                                       && ch.is_uppercase()
                                       && !last_upper {
                        words.push(buf);
                        buf = String::new();
                    }
                    last_upper = ch.is_uppercase();
                    buf.push(ch.to_lowercase());
                }
                words.push(buf);
            }
            words.connect("_")
        }

        let s = token::get_ident(ident);

        if !is_snake_case(ident) {
            cx.span_lint(NON_SNAKE_CASE, span,
                format!("{} `{}` should have a snake case name such as `{}`",
                        sort, s, to_snake_case(s.get())).as_slice());
        }
    }
}

impl LintPass for NonSnakeCase {
    fn get_lints(&self) -> LintArray {
        lint_array!(NON_SNAKE_CASE)
    }

    fn check_fn(&mut self, cx: &Context,
                fk: visit::FnKind, _: &ast::FnDecl,
                _: &ast::Block, span: Span, _: ast::NodeId) {
        match fk {
            visit::FkMethod(ident, _, m) => match method_context(cx, m) {
                PlainImpl
                    => self.check_snake_case(cx, "method", ident, span),
                TraitDefaultImpl
                    => self.check_snake_case(cx, "trait method", ident, span),
                _ => (),
            },
            visit::FkItemFn(ident, _, _, _)
                => self.check_snake_case(cx, "function", ident, span),
            _ => (),
        }
    }

    fn check_item(&mut self, cx: &Context, it: &ast::Item) {
        match it.node {
            ast::ItemMod(_) => {
                self.check_snake_case(cx, "module", it.ident, it.span);
            }
            _ => {}
        }
    }

    fn check_ty_method(&mut self, cx: &Context, t: &ast::TypeMethod) {
        self.check_snake_case(cx, "trait method", t.ident, t.span);
    }

    fn check_lifetime_decl(&mut self, cx: &Context, t: &ast::LifetimeDef) {
        self.check_snake_case(cx, "lifetime", t.lifetime.name.ident(), t.lifetime.span);
    }

    fn check_pat(&mut self, cx: &Context, p: &ast::Pat) {
        match &p.node {
            &ast::PatIdent(_, ref path1, _) => {
                match cx.tcx.def_map.borrow().get(&p.id) {
                    Some(&def::DefLocal(_)) => {
                        self.check_snake_case(cx, "variable", path1.node, p.span);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn check_struct_def(&mut self, cx: &Context, s: &ast::StructDef,
            _: ast::Ident, _: &ast::Generics, _: ast::NodeId) {
        for sf in s.fields.iter() {
            match sf.node {
                ast::StructField_ { kind: ast::NamedField(ident, _), .. } => {
                    self.check_snake_case(cx, "structure field", ident, sf.span);
                }
                _ => {}
            }
        }
    }
}

declare_lint!(pub NON_UPPER_CASE_GLOBALS, Warn,
              "static constants should have uppercase identifiers")

pub struct NonUpperCaseGlobals;

impl LintPass for NonUpperCaseGlobals {
    fn get_lints(&self) -> LintArray {
        lint_array!(NON_UPPER_CASE_GLOBALS)
    }

    fn check_item(&mut self, cx: &Context, it: &ast::Item) {
        match it.node {
            // only check static constants
            ast::ItemStatic(_, ast::MutImmutable, _) |
            ast::ItemConst(..) => {
                let s = token::get_ident(it.ident);
                // check for lowercase letters rather than non-uppercase
                // ones (some scripts don't have a concept of
                // upper/lowercase)
                if s.get().chars().any(|c| c.is_lowercase()) {
                    cx.span_lint(NON_UPPER_CASE_GLOBALS, it.span,
                        format!("static constant `{}` should have an uppercase name \
                                 such as `{}`",
                                s.get(), s.get().chars().map(|c| c.to_uppercase())
                                .collect::<String>().as_slice()).as_slice());
                }
            }
            _ => {}
        }
    }

    fn check_pat(&mut self, cx: &Context, p: &ast::Pat) {
        // Lint for constants that look like binding identifiers (#7526)
        match (&p.node, cx.tcx.def_map.borrow().get(&p.id)) {
            (&ast::PatIdent(_, ref path1, _), Some(&def::DefConst(..))) => {
                let s = token::get_ident(path1.node);
                if s.get().chars().any(|c| c.is_lowercase()) {
                    cx.span_lint(NON_UPPER_CASE_GLOBALS, path1.span,
                        format!("static constant in pattern `{}` should have an uppercase \
                                 name such as `{}`",
                                s.get(), s.get().chars().map(|c| c.to_uppercase())
                                    .collect::<String>().as_slice()).as_slice());
                }
            }
            _ => {}
        }
    }
}

declare_lint!(UNUSED_PARENS, Warn,
              "`if`, `match`, `while` and `return` do not need parentheses")

pub struct UnusedParens;

impl UnusedParens {
    fn check_unused_parens_core(&self, cx: &Context, value: &ast::Expr, msg: &str,
                                     struct_lit_needs_parens: bool) {
        match value.node {
            ast::ExprParen(ref inner) => {
                let necessary = struct_lit_needs_parens && contains_exterior_struct_lit(&**inner);
                if !necessary {
                    cx.span_lint(UNUSED_PARENS, value.span,
                                 format!("unnecessary parentheses around {}",
                                         msg).as_slice())
                }
            }
            _ => {}
        }

        /// Expressions that syntactically contain an "exterior" struct
        /// literal i.e. not surrounded by any parens or other
        /// delimiters, e.g. `X { y: 1 }`, `X { y: 1 }.method()`, `foo
        /// == X { y: 1 }` and `X { y: 1 } == foo` all do, but `(X {
        /// y: 1 }) == foo` does not.
        fn contains_exterior_struct_lit(value: &ast::Expr) -> bool {
            match value.node {
                ast::ExprStruct(..) => true,

                ast::ExprAssign(ref lhs, ref rhs) |
                ast::ExprAssignOp(_, ref lhs, ref rhs) |
                ast::ExprBinary(_, ref lhs, ref rhs) => {
                    // X { y: 1 } + X { y: 2 }
                    contains_exterior_struct_lit(&**lhs) ||
                        contains_exterior_struct_lit(&**rhs)
                }
                ast::ExprUnary(_, ref x) |
                ast::ExprCast(ref x, _) |
                ast::ExprField(ref x, _, _) |
                ast::ExprTupField(ref x, _, _) |
                ast::ExprIndex(ref x, _) => {
                    // &X { y: 1 }, X { y: 1 }.y
                    contains_exterior_struct_lit(&**x)
                }

                ast::ExprMethodCall(_, _, ref exprs) => {
                    // X { y: 1 }.bar(...)
                    contains_exterior_struct_lit(&*exprs[0])
                }

                _ => false
            }
        }
    }
}

impl LintPass for UnusedParens {
    fn get_lints(&self) -> LintArray {
        lint_array!(UNUSED_PARENS)
    }

    fn check_expr(&mut self, cx: &Context, e: &ast::Expr) {
        let (value, msg, struct_lit_needs_parens) = match e.node {
            ast::ExprIf(ref cond, _, _) => (cond, "`if` condition", true),
            ast::ExprWhile(ref cond, _, _) => (cond, "`while` condition", true),
            ast::ExprMatch(ref head, _, source) => match source {
                ast::MatchNormal => (head, "`match` head expression", true),
                ast::MatchIfLetDesugar => (head, "`if let` head expression", true),
                ast::MatchWhileLetDesugar => (head, "`while let` head expression", true),
            },
            ast::ExprRet(Some(ref value)) => (value, "`return` value", false),
            ast::ExprAssign(_, ref value) => (value, "assigned value", false),
            ast::ExprAssignOp(_, _, ref value) => (value, "assigned value", false),
            _ => return
        };
        self.check_unused_parens_core(cx, &**value, msg, struct_lit_needs_parens);
    }

    fn check_stmt(&mut self, cx: &Context, s: &ast::Stmt) {
        let (value, msg) = match s.node {
            ast::StmtDecl(ref decl, _) => match decl.node {
                ast::DeclLocal(ref local) => match local.init {
                    Some(ref value) => (value, "assigned value"),
                    None => return
                },
                _ => return
            },
            _ => return
        };
        self.check_unused_parens_core(cx, &**value, msg, false);
    }
}

declare_lint!(UNUSED_IMPORT_BRACES, Allow,
              "unnecessary braces around an imported item")

pub struct UnusedImportBraces;

impl LintPass for UnusedImportBraces {
    fn get_lints(&self) -> LintArray {
        lint_array!(UNUSED_IMPORT_BRACES)
    }

    fn check_view_item(&mut self, cx: &Context, view_item: &ast::ViewItem) {
        match view_item.node {
            ast::ViewItemUse(ref view_path) => {
                match view_path.node {
                    ast::ViewPathList(_, ref items, _) => {
                        if items.len() == 1 {
                            match items[0].node {
                                ast::PathListIdent {ref name, ..} => {
                                    let m = format!("braces around {} is unnecessary",
                                                    token::get_ident(*name).get());
                                    cx.span_lint(UNUSED_IMPORT_BRACES, view_item.span,
                                                 m.as_slice());
                                },
                                _ => ()
                            }
                        }
                    }
                    _ => ()
                }
            },
            _ => ()
        }
    }
}

declare_lint!(NON_SHORTHAND_FIELD_PATTERNS, Warn,
              "using `Struct { x: x }` instead of `Struct { x }`")

pub struct NonShorthandFieldPatterns;

impl LintPass for NonShorthandFieldPatterns {
    fn get_lints(&self) -> LintArray {
        lint_array!(NON_SHORTHAND_FIELD_PATTERNS)
    }

    fn check_pat(&mut self, cx: &Context, pat: &ast::Pat) {
        let def_map = cx.tcx.def_map.borrow();
        match pat.node {
            ast::PatStruct(_, ref v, _) => {
                for fieldpat in v.iter()
                                 .filter(|fieldpat| !fieldpat.node.is_shorthand)
                                 .filter(|fieldpat| def_map.get(&fieldpat.node.pat.id)
                                    == Some(&def::DefLocal(fieldpat.node.pat.id))) {
                    match fieldpat.node.pat.node {
                        ast::PatIdent(_, ident, None) if ident.node.as_str()
                                                         == fieldpat.node.ident.as_str() => {
                            cx.span_lint(NON_SHORTHAND_FIELD_PATTERNS, fieldpat.span,
                                         format!("the `{}:` in this pattern is redundant and can \
                                                  be removed", ident.node.as_str()).as_slice())
                        },
                        _ => {},
                    }
                }
            },
            _ => {}
        }
    }
}

declare_lint!(pub UNUSED_UNSAFE, Warn,
              "unnecessary use of an `unsafe` block")

pub struct UnusedUnsafe;

impl LintPass for UnusedUnsafe {
    fn get_lints(&self) -> LintArray {
        lint_array!(UNUSED_UNSAFE)
    }

    fn check_expr(&mut self, cx: &Context, e: &ast::Expr) {
        match e.node {
            // Don't warn about generated blocks, that'll just pollute the output.
            ast::ExprBlock(ref blk) => {
                if blk.rules == ast::UnsafeBlock(ast::UserProvided) &&
                    !cx.tcx.used_unsafe.borrow().contains(&blk.id) {
                    cx.span_lint(UNUSED_UNSAFE, blk.span, "unnecessary `unsafe` block");
                }
            }
            _ => ()
        }
    }
}

declare_lint!(UNSAFE_BLOCKS, Allow,
              "usage of an `unsafe` block")

pub struct UnsafeBlocks;

impl LintPass for UnsafeBlocks {
    fn get_lints(&self) -> LintArray {
        lint_array!(UNSAFE_BLOCKS)
    }

    fn check_expr(&mut self, cx: &Context, e: &ast::Expr) {
        match e.node {
            // Don't warn about generated blocks, that'll just pollute the output.
            ast::ExprBlock(ref blk) if blk.rules == ast::UnsafeBlock(ast::UserProvided) => {
                cx.span_lint(UNSAFE_BLOCKS, blk.span, "usage of an `unsafe` block");
            }
            _ => ()
        }
    }
}

declare_lint!(pub UNUSED_MUT, Warn,
              "detect mut variables which don't need to be mutable")

pub struct UnusedMut;

impl UnusedMut {
    fn check_unused_mut_pat(&self, cx: &Context, pats: &[P<ast::Pat>]) {
        // collect all mutable pattern and group their NodeIDs by their Identifier to
        // avoid false warnings in match arms with multiple patterns

        let mut mutables = HashMap::new();
        for p in pats.iter() {
            pat_util::pat_bindings(&cx.tcx.def_map, &**p, |mode, id, _, path1| {
                let ident = path1.node;
                match mode {
                    ast::BindByValue(ast::MutMutable) => {
                        if !token::get_ident(ident).get().starts_with("_") {
                            match mutables.entry(ident.name.uint()) {
                                Vacant(entry) => { entry.set(vec![id]); },
                                Occupied(mut entry) => { entry.get_mut().push(id); },
                            }
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
                cx.span_lint(UNUSED_MUT, cx.tcx.map.span(v[0]),
                             "variable does not need to be mutable");
            }
        }
    }
}

impl LintPass for UnusedMut {
    fn get_lints(&self) -> LintArray {
        lint_array!(UNUSED_MUT)
    }

    fn check_expr(&mut self, cx: &Context, e: &ast::Expr) {
        match e.node {
            ast::ExprMatch(_, ref arms, _) => {
                for a in arms.iter() {
                    self.check_unused_mut_pat(cx, a.pats.as_slice())
                }
            }
            _ => {}
        }
    }

    fn check_stmt(&mut self, cx: &Context, s: &ast::Stmt) {
        match s.node {
            ast::StmtDecl(ref d, _) => {
                match d.node {
                    ast::DeclLocal(ref l) => {
                        self.check_unused_mut_pat(cx, slice::ref_slice(&l.pat));
                    },
                    _ => {}
                }
            },
            _ => {}
        }
    }

    fn check_fn(&mut self, cx: &Context,
                _: visit::FnKind, decl: &ast::FnDecl,
                _: &ast::Block, _: Span, _: ast::NodeId) {
        for a in decl.inputs.iter() {
            self.check_unused_mut_pat(cx, slice::ref_slice(&a.pat));
        }
    }
}

declare_lint!(UNUSED_ALLOCATION, Warn,
              "detects unnecessary allocations that can be eliminated")

pub struct UnusedAllocation;

impl LintPass for UnusedAllocation {
    fn get_lints(&self) -> LintArray {
        lint_array!(UNUSED_ALLOCATION)
    }

    fn check_expr(&mut self, cx: &Context, e: &ast::Expr) {
        match e.node {
            ast::ExprUnary(ast::UnUniq, _) => (),
            _ => return
        }

        match cx.tcx.adjustments.borrow().get(&e.id) {
            Some(adjustment) => {
                match *adjustment {
                    ty::AdjustDerefRef(ty::AutoDerefRef { ref autoref, .. }) => {
                        match autoref {
                            &Some(ty::AutoPtr(_, ast::MutImmutable, None)) => {
                                cx.span_lint(UNUSED_ALLOCATION, e.span,
                                             "unnecessary allocation, use & instead");
                            }
                            &Some(ty::AutoPtr(_, ast::MutMutable, None)) => {
                                cx.span_lint(UNUSED_ALLOCATION, e.span,
                                             "unnecessary allocation, use &mut instead");
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
}

declare_lint!(MISSING_DOCS, Allow,
              "detects missing documentation for public members")

pub struct MissingDoc {
    /// Stack of IDs of struct definitions.
    struct_def_stack: Vec<ast::NodeId>,

    /// Stack of whether #[doc(hidden)] is set
    /// at each level which has lint attributes.
    doc_hidden_stack: Vec<bool>,
}

impl MissingDoc {
    pub fn new() -> MissingDoc {
        MissingDoc {
            struct_def_stack: vec!(),
            doc_hidden_stack: vec!(false),
        }
    }

    fn doc_hidden(&self) -> bool {
        *self.doc_hidden_stack.last().expect("empty doc_hidden_stack")
    }

    fn check_missing_docs_attrs(&self,
                               cx: &Context,
                               id: Option<ast::NodeId>,
                               attrs: &[ast::Attribute],
                               sp: Span,
                               desc: &'static str) {
        // If we're building a test harness, then warning about
        // documentation is probably not really relevant right now.
        if cx.sess().opts.test { return }

        // `#[doc(hidden)]` disables missing_docs check.
        if self.doc_hidden() { return }

        // Only check publicly-visible items, using the result from the privacy pass.
        // It's an option so the crate root can also use this function (it doesn't
        // have a NodeId).
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
            cx.span_lint(MISSING_DOCS, sp,
                format!("missing documentation for {}", desc).as_slice());
        }
    }
}

impl LintPass for MissingDoc {
    fn get_lints(&self) -> LintArray {
        lint_array!(MISSING_DOCS)
    }

    fn enter_lint_attrs(&mut self, _: &Context, attrs: &[ast::Attribute]) {
        let doc_hidden = self.doc_hidden() || attrs.iter().any(|attr| {
            attr.check_name("doc") && match attr.meta_item_list() {
                None => false,
                Some(l) => attr::contains_name(l.as_slice(), "hidden"),
            }
        });
        self.doc_hidden_stack.push(doc_hidden);
    }

    fn exit_lint_attrs(&mut self, _: &Context, _: &[ast::Attribute]) {
        self.doc_hidden_stack.pop().expect("empty doc_hidden_stack");
    }

    fn check_struct_def(&mut self, _: &Context,
        _: &ast::StructDef, _: ast::Ident, _: &ast::Generics, id: ast::NodeId) {
        self.struct_def_stack.push(id);
    }

    fn check_struct_def_post(&mut self, _: &Context,
        _: &ast::StructDef, _: ast::Ident, _: &ast::Generics, id: ast::NodeId) {
        let popped = self.struct_def_stack.pop().expect("empty struct_def_stack");
        assert!(popped == id);
    }

    fn check_crate(&mut self, cx: &Context, krate: &ast::Crate) {
        self.check_missing_docs_attrs(cx, None, krate.attrs.as_slice(),
                                     krate.span, "crate");
    }

    fn check_item(&mut self, cx: &Context, it: &ast::Item) {
        let desc = match it.node {
            ast::ItemFn(..) => "a function",
            ast::ItemMod(..) => "a module",
            ast::ItemEnum(..) => "an enum",
            ast::ItemStruct(..) => "a struct",
            ast::ItemTrait(..) => "a trait",
            _ => return
        };
        self.check_missing_docs_attrs(cx, Some(it.id), it.attrs.as_slice(),
                                     it.span, desc);
    }

    fn check_fn(&mut self, cx: &Context,
            fk: visit::FnKind, _: &ast::FnDecl,
            _: &ast::Block, _: Span, _: ast::NodeId) {
        match fk {
            visit::FkMethod(_, _, m) => {
                // If the method is an impl for a trait, don't doc.
                if method_context(cx, m) == TraitImpl { return; }

                // Otherwise, doc according to privacy. This will also check
                // doc for default methods defined on traits.
                self.check_missing_docs_attrs(cx, Some(m.id), m.attrs.as_slice(),
                                             m.span, "a method");
            }
            _ => {}
        }
    }

    fn check_ty_method(&mut self, cx: &Context, tm: &ast::TypeMethod) {
        self.check_missing_docs_attrs(cx, Some(tm.id), tm.attrs.as_slice(),
                                     tm.span, "a type method");
    }

    fn check_struct_field(&mut self, cx: &Context, sf: &ast::StructField) {
        match sf.node.kind {
            ast::NamedField(_, vis) if vis == ast::Public => {
                let cur_struct_def = *self.struct_def_stack.last()
                    .expect("empty struct_def_stack");
                self.check_missing_docs_attrs(cx, Some(cur_struct_def),
                                             sf.node.attrs.as_slice(), sf.span,
                                             "a struct field")
            }
            _ => {}
        }
    }

    fn check_variant(&mut self, cx: &Context, v: &ast::Variant, _: &ast::Generics) {
        self.check_missing_docs_attrs(cx, Some(v.node.id), v.node.attrs.as_slice(),
                                     v.span, "a variant");
    }
}

declare_lint!(DEPRECATED, Warn,
              "detects use of #[deprecated] items")

// FIXME #6875: Change to Warn after std library stabilization is complete
declare_lint!(EXPERIMENTAL, Allow,
              "detects use of #[experimental] items")

declare_lint!(UNSTABLE, Allow,
              "detects use of #[unstable] items (incl. items with no stability attribute)")

/// Checks for use of items with `#[deprecated]`, `#[experimental]` and
/// `#[unstable]` attributes, or no stability attribute.
pub struct Stability;

impl Stability {
    fn lint(&self, cx: &Context, id: ast::DefId, span: Span) {
        let stability = stability::lookup(cx.tcx, id);
        let cross_crate = !ast_util::is_local(id);

        // stability attributes are promises made across crates; only
        // check DEPRECATED for crate-local usage.
        let (lint, label) = match stability {
            // no stability attributes == Unstable
            None if cross_crate => (UNSTABLE, "unmarked"),
            Some(attr::Stability { level: attr::Unstable, .. }) if cross_crate =>
                (UNSTABLE, "unstable"),
            Some(attr::Stability { level: attr::Experimental, .. }) if cross_crate =>
                (EXPERIMENTAL, "experimental"),
            Some(attr::Stability { level: attr::Deprecated, .. }) =>
                (DEPRECATED, "deprecated"),
            _ => return
        };

        let msg = match stability {
            Some(attr::Stability { text: Some(ref s), .. }) => {
                format!("use of {} item: {}", label, *s)
            }
            _ => format!("use of {} item", label)
        };

        cx.span_lint(lint, span, msg.as_slice());
    }

    fn is_internal(&self, cx: &Context, span: Span) -> bool {
        // first, check if the given expression was generated by a macro or not
        // we need to go back the expn_info tree to check only the arguments
        // of the initial macro call, not the nested ones.
        let mut expnid = span.expn_id;
        let mut is_internal = false;
        while cx.tcx.sess.codemap().with_expn_info(expnid, |expninfo| {
            match expninfo {
                Some(ref info) => {
                    // save the parent expn_id for next loop iteration
                    expnid = info.call_site.expn_id;
                    if info.callee.span.is_none() {
                        // it's a compiler built-in, we *really* don't want to mess with it
                        // so we skip it, unless it was called by a regular macro, in which case
                        // we will handle the caller macro next turn
                        is_internal = true;
                        true // continue looping
                    } else {
                        // was this expression from the current macro arguments ?
                        is_internal = !( span.lo > info.call_site.lo &&
                                         span.hi < info.call_site.hi );
                        true // continue looping
                    }
                },
                _ => false // stop looping
            }
        }) { /* empty while loop body */ }
        return is_internal;
    }
}

impl LintPass for Stability {
    fn get_lints(&self) -> LintArray {
        lint_array!(DEPRECATED, EXPERIMENTAL, UNSTABLE)
    }

    fn check_view_item(&mut self, cx: &Context, item: &ast::ViewItem) {
        // compiler-generated `extern crate` statements have a dummy span.
        if item.span == DUMMY_SP { return }

        let id = match item.node {
            ast::ViewItemExternCrate(_, _, id) => id,
            ast::ViewItemUse(..) => return,
        };
        let cnum = match cx.tcx.sess.cstore.find_extern_mod_stmt_cnum(id) {
            Some(cnum) => cnum,
            None => return,
        };
        let id = ast::DefId { krate: cnum, node: ast::CRATE_NODE_ID };
        self.lint(cx, id, item.span);
    }

    fn check_expr(&mut self, cx: &Context, e: &ast::Expr) {
        if self.is_internal(cx, e.span) { return; }

        let mut span = e.span;

        let id = match e.node {
            ast::ExprPath(..) | ast::ExprStruct(..) => {
                match cx.tcx.def_map.borrow().get(&e.id) {
                    Some(&def) => def.def_id(),
                    None => return
                }
            }
            ast::ExprMethodCall(i, _, _) => {
                span = i.span;
                let method_call = typeck::MethodCall::expr(e.id);
                match cx.tcx.method_map.borrow().get(&method_call) {
                    Some(method) => {
                        match method.origin {
                            typeck::MethodStatic(def_id) => {
                                def_id
                            }
                            typeck::MethodStaticUnboxedClosure(def_id) => {
                                def_id
                            }
                            typeck::MethodTypeParam(typeck::MethodParam {
                                ref trait_ref,
                                method_num: index,
                                ..
                            }) |
                            typeck::MethodTraitObject(typeck::MethodObject {
                                ref trait_ref,
                                method_num: index,
                                ..
                            }) => {
                                ty::trait_item(cx.tcx,
                                               trait_ref.def_id,
                                               index).def_id()
                            }
                        }
                    }
                    None => return
                }
            }
            _ => return
        };
        self.lint(cx, id, span);
    }

    fn check_item(&mut self, cx: &Context, item: &ast::Item) {
        if self.is_internal(cx, item.span) { return }

        match item.node {
            ast::ItemTrait(_, _, ref supertraits, _) => {
                for t in supertraits.iter() {
                    match *t {
                        ast::TraitTyParamBound(ref t) => {
                            let id = ty::trait_ref_to_def_id(cx.tcx, t);
                            self.lint(cx, id, t.path.span);
                        }
                        _ => (/* pass */)
                    }
                }
            }
            ast::ItemImpl(_, Some(ref t), _, _) => {
                let id = ty::trait_ref_to_def_id(cx.tcx, t);
                self.lint(cx, id, t.path.span);
            }
            _ => (/* pass */)
        }
    }
}

declare_lint!(pub UNUSED_IMPORTS, Warn,
              "imports that are never used")

declare_lint!(pub UNUSED_EXTERN_CRATES, Allow,
              "extern crates that are never used")

declare_lint!(pub UNUSED_QUALIFICATIONS, Allow,
              "detects unnecessarily qualified names")

declare_lint!(pub UNKNOWN_LINTS, Warn,
              "unrecognized lint attribute")

declare_lint!(pub UNUSED_VARIABLES, Warn,
              "detect variables which are not used in any way")

declare_lint!(pub UNUSED_ASSIGNMENTS, Warn,
              "detect assignments that will never be read")

declare_lint!(pub DEAD_CODE, Warn,
              "detect unused, unexported items")

declare_lint!(pub UNREACHABLE_CODE, Warn,
              "detects unreachable code paths")

declare_lint!(pub WARNINGS, Warn,
              "mass-change the level for lints which produce warnings")

declare_lint!(pub UNKNOWN_FEATURES, Deny,
              "unknown features found in crate-level #[feature] directives")

declare_lint!(pub UNKNOWN_CRATE_TYPES, Deny,
              "unknown crate type found in #[crate_type] directive")

declare_lint!(pub VARIANT_SIZE_DIFFERENCES, Allow,
              "detects enums with widely varying variant sizes")

declare_lint!(pub FAT_PTR_TRANSMUTES, Allow,
              "detects transmutes of fat pointers")

/// Does nothing as a lint pass, but registers some `Lint`s
/// which are used by other parts of the compiler.
pub struct HardwiredLints;

impl LintPass for HardwiredLints {
    fn get_lints(&self) -> LintArray {
        lint_array!(
            UNUSED_IMPORTS,
            UNUSED_EXTERN_CRATES,
            UNUSED_QUALIFICATIONS,
            UNKNOWN_LINTS,
            UNUSED_VARIABLES,
            UNUSED_ASSIGNMENTS,
            DEAD_CODE,
            UNREACHABLE_CODE,
            WARNINGS,
            UNKNOWN_FEATURES,
            UNKNOWN_CRATE_TYPES,
            VARIANT_SIZE_DIFFERENCES,
            FAT_PTR_TRANSMUTES
        )
    }
}
