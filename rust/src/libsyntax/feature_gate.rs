// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Feature gating
//!
//! This modules implements the gating necessary for preventing certain compiler
//! features from being used by default. This module will crawl a pre-expanded
//! AST to ensure that there are no features which are used that are not
//! enabled.
//!
//! Features are enabled in programs via the crate-level attributes of
//! `#![feature(...)]` with a comma-separated list of features.
use self::Status::*;

use abi::RustIntrinsic;
use ast::NodeId;
use ast;
use attr;
use attr::AttrMetaMethods;
use codemap::{CodeMap, Span};
use diagnostic::SpanHandler;
use visit;
use visit::Visitor;
use parse::token;

use std::slice;
use std::ascii::AsciiExt;


// if you change this list without updating src/doc/reference.md, @cmr will be sad
static KNOWN_FEATURES: &'static [(&'static str, Status)] = &[
    ("globs", Active),
    ("macro_rules", Active),
    ("struct_variant", Accepted),
    ("asm", Active),
    ("managed_boxes", Removed),
    ("non_ascii_idents", Active),
    ("thread_local", Active),
    ("link_args", Active),
    ("phase", Active),
    ("plugin_registrar", Active),
    ("log_syntax", Active),
    ("trace_macros", Active),
    ("concat_idents", Active),
    ("unsafe_destructor", Active),
    ("intrinsics", Active),
    ("lang_items", Active),

    ("simd", Active),
    ("default_type_params", Active),
    ("quote", Active),
    ("link_llvm_intrinsics", Active),
    ("linkage", Active),
    ("struct_inherit", Removed),

    ("quad_precision_float", Removed),

    ("rustc_diagnostic_macros", Active),
    ("unboxed_closures", Active),
    ("import_shadowing", Active),
    ("advanced_slice_patterns", Active),
    ("tuple_indexing", Accepted),
    ("associated_types", Active),
    ("visible_private_types", Active),
    ("slicing_syntax", Active),

    ("if_let", Accepted),
    ("while_let", Accepted),

    // A temporary feature gate used to enable parser extensions needed
    // to bootstrap fix for #5723.
    ("issue_5723_bootstrap", Accepted),

    // A way to temporarily opt out of opt in copy. This will *never* be accepted.
    ("opt_out_copy", Deprecated),

    // A way to temporarily opt out of the new orphan rules. This will *never* be accepted.
    ("old_orphan_check", Deprecated),

    // These are used to test this portion of the compiler, they don't actually
    // mean anything
    ("test_accepted_feature", Accepted),
    ("test_removed_feature", Removed),
];

enum Status {
    /// Represents an active feature that is currently being implemented or
    /// currently being considered for addition/removal.
    Active,

    /// Represents a feature gate that is temporarily enabling deprecated behavior.
    /// This gate will never be accepted.
    Deprecated,

    /// Represents a feature which has since been removed (it was once Active)
    Removed,

    /// This language feature has since been Accepted (it was once Active)
    Accepted,
}

/// A set of features to be used by later passes.
#[deriving(Copy)]
pub struct Features {
    pub default_type_params: bool,
    pub unboxed_closures: bool,
    pub rustc_diagnostic_macros: bool,
    pub import_shadowing: bool,
    pub visible_private_types: bool,
    pub quote: bool,
    pub opt_out_copy: bool,
    pub old_orphan_check: bool,
}

impl Features {
    pub fn new() -> Features {
        Features {
            default_type_params: false,
            unboxed_closures: false,
            rustc_diagnostic_macros: false,
            import_shadowing: false,
            visible_private_types: false,
            quote: false,
            opt_out_copy: false,
            old_orphan_check: false,
        }
    }
}

struct Context<'a> {
    features: Vec<&'static str>,
    span_handler: &'a SpanHandler,
    cm: &'a CodeMap,
}

impl<'a> Context<'a> {
    fn gate_feature(&self, feature: &str, span: Span, explain: &str) {
        if !self.has_feature(feature) {
            self.span_handler.span_err(span, explain);
            self.span_handler.span_help(span, format!("add #![feature({})] to the \
                                                       crate attributes to enable",
                                                      feature)[]);
        }
    }

    fn has_feature(&self, feature: &str) -> bool {
        self.features.iter().any(|&n| n == feature)
    }
}

struct MacroVisitor<'a> {
    context: &'a Context<'a>
}

impl<'a, 'v> Visitor<'v> for MacroVisitor<'a> {
    fn visit_view_item(&mut self, i: &ast::ViewItem) {
        match i.node {
            ast::ViewItemExternCrate(..) => {
                for attr in i.attrs.iter() {
                    if attr.name().get() == "phase"{
                        self.context.gate_feature("phase", attr.span,
                                          "compile time crate loading is \
                                           experimental and possibly buggy");
                    }
                }
            },
            _ => { }
        }
        visit::walk_view_item(self, i)
    }

    fn visit_mac(&mut self, macro: &ast::Mac) {
        let ast::MacInvocTT(ref path, _, _) = macro.node;
        let id = path.segments.last().unwrap().identifier;

        if id == token::str_to_ident("macro_rules") {
            self.context.gate_feature("macro_rules", path.span, "macro definitions are \
                not stable enough for use and are subject to change");
        }

        else if id == token::str_to_ident("asm") {
            self.context.gate_feature("asm", path.span, "inline assembly is not \
                stable enough for use and is subject to change");
        }

        else if id == token::str_to_ident("log_syntax") {
            self.context.gate_feature("log_syntax", path.span, "`log_syntax!` is not \
                stable enough for use and is subject to change");
        }

        else if id == token::str_to_ident("trace_macros") {
            self.context.gate_feature("trace_macros", path.span, "`trace_macros` is not \
                stable enough for use and is subject to change");
        }

        else if id == token::str_to_ident("concat_idents") {
            self.context.gate_feature("concat_idents", path.span, "`concat_idents` is not \
                stable enough for use and is subject to change");
        }
    }
}

struct PostExpansionVisitor<'a> {
    context: &'a Context<'a>
}

impl<'a> PostExpansionVisitor<'a> {
    fn gate_feature(&self, feature: &str, span: Span, explain: &str) {
        if !self.context.cm.span_is_internal(span) {
            self.context.gate_feature(feature, span, explain)
        }
    }
}

impl<'a, 'v> Visitor<'v> for PostExpansionVisitor<'a> {
    fn visit_name(&mut self, sp: Span, name: ast::Name) {
        if !token::get_name(name).get().is_ascii() {
            self.gate_feature("non_ascii_idents", sp,
                              "non-ascii idents are not fully supported.");
        }
    }

    fn visit_view_item(&mut self, i: &ast::ViewItem) {
        match i.node {
            ast::ViewItemUse(ref path) => {
                if let ast::ViewPathGlob(..) = path.node {
                    self.gate_feature("globs", path.span,
                                      "glob import statements are \
                                       experimental and possibly buggy");
                }
            }
            ast::ViewItemExternCrate(..) => {
                for attr in i.attrs.iter() {
                    if attr.name().get() == "phase"{
                        self.gate_feature("phase", attr.span,
                                          "compile time crate loading is \
                                           experimental and possibly buggy");
                    }
                }
            }
        }
        visit::walk_view_item(self, i)
    }

    fn visit_item(&mut self, i: &ast::Item) {
        for attr in i.attrs.iter() {
            if attr.name() == "thread_local" {
                self.gate_feature("thread_local", i.span,
                                  "`#[thread_local]` is an experimental feature, and does not \
                                  currently handle destructors. There is no corresponding \
                                  `#[task_local]` mapping to the task model");
            } else if attr.name() == "linkage" {
                self.gate_feature("linkage", i.span,
                                  "the `linkage` attribute is experimental \
                                   and not portable across platforms")
            }
        }
        match i.node {
            ast::ItemForeignMod(ref foreign_module) => {
                if attr::contains_name(i.attrs[], "link_args") {
                    self.gate_feature("link_args", i.span,
                                      "the `link_args` attribute is not portable \
                                       across platforms, it is recommended to \
                                       use `#[link(name = \"foo\")]` instead")
                }
                if foreign_module.abi == RustIntrinsic {
                    self.gate_feature("intrinsics",
                                      i.span,
                                      "intrinsics are subject to change")
                }
            }

            ast::ItemFn(..) => {
                if attr::contains_name(i.attrs[], "plugin_registrar") {
                    self.gate_feature("plugin_registrar", i.span,
                                      "compiler plugins are experimental and possibly buggy");
                }
            }

            ast::ItemStruct(..) => {
                if attr::contains_name(i.attrs[], "simd") {
                    self.gate_feature("simd", i.span,
                                      "SIMD types are experimental and possibly buggy");
                }
            }

            ast::ItemImpl(_, _, _, _, ref items) => {
                if attr::contains_name(i.attrs[],
                                       "unsafe_destructor") {
                    self.gate_feature("unsafe_destructor",
                                      i.span,
                                      "`#[unsafe_destructor]` allows too \
                                       many unsafe patterns and may be \
                                       removed in the future");
                }

                for item in items.iter() {
                    match *item {
                        ast::MethodImplItem(_) => {}
                        ast::TypeImplItem(ref typedef) => {
                            self.gate_feature("associated_types",
                                              typedef.span,
                                              "associated types are \
                                               experimental")
                        }
                    }
                }
            }

            _ => {}
        }

        visit::walk_item(self, i);
    }

    fn visit_trait_item(&mut self, trait_item: &ast::TraitItem) {
        match *trait_item {
            ast::RequiredMethod(_) | ast::ProvidedMethod(_) => {}
            ast::TypeTraitItem(ref ti) => {
                self.gate_feature("associated_types",
                                  ti.ty_param.span,
                                  "associated types are experimental")
            }
        }
    }

    fn visit_foreign_item(&mut self, i: &ast::ForeignItem) {
        if attr::contains_name(i.attrs[], "linkage") {
            self.gate_feature("linkage", i.span,
                              "the `linkage` attribute is experimental \
                               and not portable across platforms")
        }

        let links_to_llvm = match attr::first_attr_value_str_by_name(i.attrs[], "link_name") {
            Some(val) => val.get().starts_with("llvm."),
            _ => false
        };
        if links_to_llvm {
            self.gate_feature("link_llvm_intrinsics", i.span,
                              "linking to LLVM intrinsics is experimental");
        }

        visit::walk_foreign_item(self, i)
    }

    fn visit_ty(&mut self, t: &ast::Ty) {
        if let ast::TyClosure(ref closure) =  t.node {
            // this used to be blocked by a feature gate, but it should just
            // be plain impossible right now
            assert!(closure.onceness != ast::Once);
        }

        visit::walk_ty(self, t);
    }

    fn visit_expr(&mut self, e: &ast::Expr) {
        match e.node {
            ast::ExprRange(..) => {
                self.gate_feature("slicing_syntax",
                                  e.span,
                                  "range syntax is experimental");
            }
            _ => {}
        }
        visit::walk_expr(self, e);
    }

    fn visit_generics(&mut self, generics: &ast::Generics) {
        for type_parameter in generics.ty_params.iter() {
            match type_parameter.default {
                Some(ref ty) => {
                    self.gate_feature("default_type_params", ty.span,
                                      "default type parameters are \
                                       experimental and possibly buggy");
                }
                None => {}
            }
        }
        visit::walk_generics(self, generics);
    }

    fn visit_attribute(&mut self, attr: &ast::Attribute) {
        if attr::contains_name(slice::ref_slice(attr), "lang") {
            self.gate_feature("lang_items",
                              attr.span,
                              "language items are subject to change");
        }
    }

    fn visit_pat(&mut self, pattern: &ast::Pat) {
        match pattern.node {
            ast::PatVec(_, Some(_), ref last) if !last.is_empty() => {
                self.gate_feature("advanced_slice_patterns",
                                  pattern.span,
                                  "multiple-element slice matches anywhere \
                                   but at the end of a slice (e.g. \
                                   `[0, ..xs, 0]` are experimental")
            }
            _ => {}
        }
        visit::walk_pat(self, pattern)
    }

    fn visit_fn(&mut self,
                fn_kind: visit::FnKind<'v>,
                fn_decl: &'v ast::FnDecl,
                block: &'v ast::Block,
                span: Span,
                _node_id: NodeId) {
        match fn_kind {
            visit::FkItemFn(_, _, _, abi) if abi == RustIntrinsic => {
                self.gate_feature("intrinsics",
                                  span,
                                  "intrinsics are subject to change")
            }
            _ => {}
        }
        visit::walk_fn(self, fn_kind, fn_decl, block, span);
    }
}

fn check_crate_inner<F>(cm: &CodeMap, span_handler: &SpanHandler, krate: &ast::Crate,
                        check: F)
                       -> (Features, Vec<Span>)
    where F: FnOnce(&mut Context, &ast::Crate)
{
    let mut cx = Context {
        features: Vec::new(),
        span_handler: span_handler,
        cm: cm,
    };

    let mut unknown_features = Vec::new();

    for attr in krate.attrs.iter() {
        if !attr.check_name("feature") {
            continue
        }

        match attr.meta_item_list() {
            None => {
                span_handler.span_err(attr.span, "malformed feature attribute, \
                                                  expected #![feature(...)]");
            }
            Some(list) => {
                for mi in list.iter() {
                    let name = match mi.node {
                        ast::MetaWord(ref word) => (*word).clone(),
                        _ => {
                            span_handler.span_err(mi.span,
                                                  "malformed feature, expected just \
                                                   one word");
                            continue
                        }
                    };
                    match KNOWN_FEATURES.iter()
                                        .find(|& &(n, _)| name == n) {
                        Some(&(name, Active)) => {
                            cx.features.push(name);
                        }
                        Some(&(name, Deprecated)) => {
                            cx.features.push(name);
                            span_handler.span_warn(
                                mi.span,
                                "feature is deprecated and will only be available \
                                 for a limited time, please rewrite code that relies on it");
                        }
                        Some(&(_, Removed)) => {
                            span_handler.span_err(mi.span, "feature has been removed");
                        }
                        Some(&(_, Accepted)) => {
                            span_handler.span_warn(mi.span, "feature has been added to Rust, \
                                                             directive not necessary");
                        }
                        None => {
                            unknown_features.push(mi.span);
                        }
                    }
                }
            }
        }
    }

    check(&mut cx, krate);

    (Features {
        default_type_params: cx.has_feature("default_type_params"),
        unboxed_closures: cx.has_feature("unboxed_closures"),
        rustc_diagnostic_macros: cx.has_feature("rustc_diagnostic_macros"),
        import_shadowing: cx.has_feature("import_shadowing"),
        visible_private_types: cx.has_feature("visible_private_types"),
        quote: cx.has_feature("quote"),
        opt_out_copy: cx.has_feature("opt_out_copy"),
        old_orphan_check: cx.has_feature("old_orphan_check"),
    },
    unknown_features)
}

pub fn check_crate_macros(cm: &CodeMap, span_handler: &SpanHandler, krate: &ast::Crate)
-> (Features, Vec<Span>) {
    check_crate_inner(cm, span_handler, krate,
                      |ctx, krate| visit::walk_crate(&mut MacroVisitor { context: ctx }, krate))
}

pub fn check_crate(cm: &CodeMap, span_handler: &SpanHandler, krate: &ast::Crate)
-> (Features, Vec<Span>) {
    check_crate_inner(cm, span_handler, krate,
                      |ctx, krate| visit::walk_crate(&mut PostExpansionVisitor { context: ctx },
                                                     krate))
}
