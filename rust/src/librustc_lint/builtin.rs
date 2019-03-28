//! Lints in the Rust compiler.
//!
//! This contains lints which can feasibly be implemented as their own
//! AST visitor. Also see `rustc::lint::builtin`, which contains the
//! definitions of lints that are emitted directly inside the main
//! compiler.
//!
//! To add a new lint to rustc, declare it here using `declare_lint!()`.
//! Then add code to emit the new lint in the appropriate circumstances.
//! You can do that in an existing `LintPass` if it makes sense, or in a
//! new `LintPass`, or using `Session::add_lint` elsewhere in the
//! compiler. Only do the latter if the check can't be written cleanly as a
//! `LintPass` (also, note that such lints will need to be defined in
//! `rustc::lint::builtin`, not here).
//!
//! If you define a new `EarlyLintPass`, you will also need to add it to the
//! `add_early_builtin!` or `add_early_builtin_with_new!` invocation in
//! `lib.rs`. Use the former for unit-like structs and the latter for structs
//! with a `pub fn new()`.
//!
//! If you define a new `LateLintPass`, you will also need to add it to the
//! `late_lint_methods!` invocation in `lib.rs`.

use rustc::hir::def::Def;
use rustc::hir::def_id::{DefId, LOCAL_CRATE};
use rustc::ty::{self, Ty};
use rustc::{lint, util};
use hir::Node;
use util::nodemap::HirIdSet;
use lint::{LateContext, LintContext, LintArray};
use lint::{LintPass, LateLintPass, EarlyLintPass, EarlyContext};

use rustc::util::nodemap::FxHashSet;

use syntax::tokenstream::{TokenTree, TokenStream};
use syntax::ast;
use syntax::ptr::P;
use syntax::ast::Expr;
use syntax::attr::{self, HasAttrs};
use syntax::source_map::Spanned;
use syntax::edition::Edition;
use syntax::feature_gate::{AttributeGate, AttributeTemplate, AttributeType};
use syntax::feature_gate::{Stability, deprecated_attributes};
use syntax_pos::{BytePos, Span, SyntaxContext};
use syntax::symbol::keywords;
use syntax::errors::{Applicability, DiagnosticBuilder};
use syntax::print::pprust::expr_to_string;
use syntax::visit::FnKind;
use syntax::struct_span_err;

use rustc::hir::{self, GenericParamKind, PatKind};

use crate::nonstandard_style::{MethodLateContext, method_context};

use log::debug;

// hardwired lints from librustc
pub use lint::builtin::*;

declare_lint! {
    WHILE_TRUE,
    Warn,
    "suggest using `loop { }` instead of `while true { }`"
}

#[derive(Copy, Clone)]
pub struct WhileTrue;

impl LintPass for WhileTrue {
    fn name(&self) -> &'static str {
        "WhileTrue"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!(WHILE_TRUE)
    }
}

impl<'a, 'tcx> LateLintPass<'a, 'tcx> for WhileTrue {
    fn check_expr(&mut self, cx: &LateContext<'_, '_>, e: &hir::Expr) {
        if let hir::ExprKind::While(ref cond, ..) = e.node {
            if let hir::ExprKind::Lit(ref lit) = cond.node {
                if let ast::LitKind::Bool(true) = lit.node {
                    if lit.span.ctxt() == SyntaxContext::empty() {
                        let msg = "denote infinite loops with `loop { ... }`";
                        let condition_span = cx.tcx.sess.source_map().def_span(e.span);
                        let mut err = cx.struct_span_lint(WHILE_TRUE, condition_span, msg);
                        err.span_suggestion_short(
                            condition_span,
                            "use `loop`",
                            "loop".to_owned(),
                            Applicability::MachineApplicable
                        );
                        err.emit();
                    }
                }
            }
        }
    }
}

declare_lint! {
    BOX_POINTERS,
    Allow,
    "use of owned (Box type) heap memory"
}

#[derive(Copy, Clone)]
pub struct BoxPointers;

impl BoxPointers {
    fn check_heap_type<'a, 'tcx>(&self, cx: &LateContext<'_, '_>, span: Span, ty: Ty<'_>) {
        for leaf_ty in ty.walk() {
            if leaf_ty.is_box() {
                let m = format!("type uses owned (Box type) pointers: {}", ty);
                cx.span_lint(BOX_POINTERS, span, &m);
            }
        }
    }
}

impl LintPass for BoxPointers {
    fn name(&self) -> &'static str {
        "BoxPointers"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!(BOX_POINTERS)
    }
}

impl<'a, 'tcx> LateLintPass<'a, 'tcx> for BoxPointers {
    fn check_item(&mut self, cx: &LateContext<'_, '_>, it: &hir::Item) {
        match it.node {
            hir::ItemKind::Fn(..) |
            hir::ItemKind::Ty(..) |
            hir::ItemKind::Enum(..) |
            hir::ItemKind::Struct(..) |
            hir::ItemKind::Union(..) => {
                let def_id = cx.tcx.hir().local_def_id_from_hir_id(it.hir_id);
                self.check_heap_type(cx, it.span, cx.tcx.type_of(def_id))
            }
            _ => ()
        }

        // If it's a struct, we also have to check the fields' types
        match it.node {
            hir::ItemKind::Struct(ref struct_def, _) |
            hir::ItemKind::Union(ref struct_def, _) => {
                for struct_field in struct_def.fields() {
                    let def_id = cx.tcx.hir().local_def_id_from_hir_id(struct_field.hir_id);
                    self.check_heap_type(cx, struct_field.span,
                                         cx.tcx.type_of(def_id));
                }
            }
            _ => (),
        }
    }

    fn check_expr(&mut self, cx: &LateContext<'_, '_>, e: &hir::Expr) {
        let ty = cx.tables.node_type(e.hir_id);
        self.check_heap_type(cx, e.span, ty);
    }
}

declare_lint! {
    NON_SHORTHAND_FIELD_PATTERNS,
    Warn,
    "using `Struct { x: x }` instead of `Struct { x }` in a pattern"
}

#[derive(Copy, Clone)]
pub struct NonShorthandFieldPatterns;

impl LintPass for NonShorthandFieldPatterns {
    fn name(&self) -> &'static str {
        "NonShorthandFieldPatterns"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!(NON_SHORTHAND_FIELD_PATTERNS)
    }
}

impl<'a, 'tcx> LateLintPass<'a, 'tcx> for NonShorthandFieldPatterns {
    fn check_pat(&mut self, cx: &LateContext<'_, '_>, pat: &hir::Pat) {
        if let PatKind::Struct(ref qpath, ref field_pats, _) = pat.node {
            let variant = cx.tables.pat_ty(pat).ty_adt_def()
                                   .expect("struct pattern type is not an ADT")
                                   .variant_of_def(cx.tables.qpath_def(qpath, pat.hir_id));
            for fieldpat in field_pats {
                if fieldpat.node.is_shorthand {
                    continue;
                }
                if fieldpat.span.ctxt().outer().expn_info().is_some() {
                    // Don't lint if this is a macro expansion: macro authors
                    // shouldn't have to worry about this kind of style issue
                    // (Issue #49588)
                    continue;
                }
                if let PatKind::Binding(_, _, ident, None) = fieldpat.node.pat.node {
                    if cx.tcx.find_field_index(ident, &variant) ==
                       Some(cx.tcx.field_index(fieldpat.node.hir_id, cx.tables)) {
                        let mut err = cx.struct_span_lint(NON_SHORTHAND_FIELD_PATTERNS,
                                     fieldpat.span,
                                     &format!("the `{}:` in this pattern is redundant", ident));
                        let subspan = cx.tcx.sess.source_map().span_through_char(fieldpat.span,
                                                                                 ':');
                        err.span_suggestion_short(
                            subspan,
                            "remove this",
                            ident.to_string(),
                            Applicability::MachineApplicable
                        );
                        err.emit();
                    }
                }
            }
        }
    }
}

declare_lint! {
    UNSAFE_CODE,
    Allow,
    "usage of `unsafe` code"
}

#[derive(Copy, Clone)]
pub struct UnsafeCode;

impl LintPass for UnsafeCode {
    fn name(&self) -> &'static str {
        "UnsafeCode"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!(UNSAFE_CODE)
    }
}

impl UnsafeCode {
    fn report_unsafe(&self, cx: &EarlyContext<'_>, span: Span, desc: &'static str) {
        // This comes from a macro that has #[allow_internal_unsafe].
        if span.allows_unsafe() {
            return;
        }

        cx.span_lint(UNSAFE_CODE, span, desc);
    }
}

impl EarlyLintPass for UnsafeCode {
    fn check_attribute(&mut self, cx: &EarlyContext<'_>, attr: &ast::Attribute) {
        if attr.check_name("allow_internal_unsafe") {
            self.report_unsafe(cx, attr.span, "`allow_internal_unsafe` allows defining \
                                               macros using unsafe without triggering \
                                               the `unsafe_code` lint at their call site");
        }
    }

    fn check_expr(&mut self, cx: &EarlyContext<'_>, e: &ast::Expr) {
        if let ast::ExprKind::Block(ref blk, _) = e.node {
            // Don't warn about generated blocks, that'll just pollute the output.
            if blk.rules == ast::BlockCheckMode::Unsafe(ast::UserProvided) {
                self.report_unsafe(cx, blk.span, "usage of an `unsafe` block");
            }
        }
    }

    fn check_item(&mut self, cx: &EarlyContext<'_>, it: &ast::Item) {
        match it.node {
            ast::ItemKind::Trait(_, ast::Unsafety::Unsafe, ..) => {
                self.report_unsafe(cx, it.span, "declaration of an `unsafe` trait")
            }

            ast::ItemKind::Impl(ast::Unsafety::Unsafe, ..) => {
                self.report_unsafe(cx, it.span, "implementation of an `unsafe` trait")
            }

            _ => return,
        }
    }

    fn check_fn(&mut self,
                cx: &EarlyContext<'_>,
                fk: FnKind<'_>,
                _: &ast::FnDecl,
                span: Span,
                _: ast::NodeId) {
        match fk {
            FnKind::ItemFn(_, ast::FnHeader { unsafety: ast::Unsafety::Unsafe, .. }, ..) => {
                self.report_unsafe(cx, span, "declaration of an `unsafe` function")
            }

            FnKind::Method(_, sig, ..) => {
                if sig.header.unsafety == ast::Unsafety::Unsafe {
                    self.report_unsafe(cx, span, "implementation of an `unsafe` method")
                }
            }

            _ => (),
        }
    }

    fn check_trait_item(&mut self, cx: &EarlyContext<'_>, item: &ast::TraitItem) {
        if let ast::TraitItemKind::Method(ref sig, None) = item.node {
            if sig.header.unsafety == ast::Unsafety::Unsafe {
                self.report_unsafe(cx, item.span, "declaration of an `unsafe` method")
            }
        }
    }
}

declare_lint! {
    pub MISSING_DOCS,
    Allow,
    "detects missing documentation for public members",
    report_in_external_macro: true
}

pub struct MissingDoc {
    /// Stack of whether `#[doc(hidden)]` is set at each level which has lint attributes.
    doc_hidden_stack: Vec<bool>,

    /// Private traits or trait items that leaked through. Don't check their methods.
    private_traits: FxHashSet<hir::HirId>,
}

fn has_doc(attr: &ast::Attribute) -> bool {
    if !attr.check_name("doc") {
        return false;
    }

    if attr.is_value_str() {
        return true;
    }

    if let Some(list) = attr.meta_item_list() {
        for meta in list {
            if meta.check_name("include") || meta.check_name("hidden") {
                return true;
            }
        }
    }

    false
}

impl MissingDoc {
    pub fn new() -> MissingDoc {
        MissingDoc {
            doc_hidden_stack: vec![false],
            private_traits: FxHashSet::default(),
        }
    }

    fn doc_hidden(&self) -> bool {
        *self.doc_hidden_stack.last().expect("empty doc_hidden_stack")
    }

    fn check_missing_docs_attrs(&self,
                                cx: &LateContext<'_, '_>,
                                id: Option<hir::HirId>,
                                attrs: &[ast::Attribute],
                                sp: Span,
                                desc: &'static str) {
        // If we're building a test harness, then warning about
        // documentation is probably not really relevant right now.
        if cx.sess().opts.test {
            return;
        }

        // `#[doc(hidden)]` disables missing_docs check.
        if self.doc_hidden() {
            return;
        }

        // Only check publicly-visible items, using the result from the privacy pass.
        // It's an option so the crate root can also use this function (it doesn't
        // have a NodeId).
        if let Some(id) = id {
            if !cx.access_levels.is_exported(id) {
                return;
            }
        }

        let has_doc = attrs.iter().any(|a| has_doc(a));
        if !has_doc {
            cx.span_lint(MISSING_DOCS,
                         cx.tcx.sess.source_map().def_span(sp),
                         &format!("missing documentation for {}", desc));
        }
    }
}

impl LintPass for MissingDoc {
    fn name(&self) -> &'static str {
        "MissingDoc"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!(MISSING_DOCS)
    }
}

impl<'a, 'tcx> LateLintPass<'a, 'tcx> for MissingDoc {
    fn enter_lint_attrs(&mut self, _: &LateContext<'_, '_>, attrs: &[ast::Attribute]) {
        let doc_hidden = self.doc_hidden() ||
                         attrs.iter().any(|attr| {
            attr.check_name("doc") &&
            match attr.meta_item_list() {
                None => false,
                Some(l) => attr::list_contains_name(&l, "hidden"),
            }
        });
        self.doc_hidden_stack.push(doc_hidden);
    }

    fn exit_lint_attrs(&mut self, _: &LateContext<'_, '_>, _attrs: &[ast::Attribute]) {
        self.doc_hidden_stack.pop().expect("empty doc_hidden_stack");
    }

    fn check_crate(&mut self, cx: &LateContext<'_, '_>, krate: &hir::Crate) {
        self.check_missing_docs_attrs(cx, None, &krate.attrs, krate.span, "crate");

        for macro_def in &krate.exported_macros {
            let has_doc = macro_def.attrs.iter().any(|a| has_doc(a));
            if !has_doc {
                cx.span_lint(MISSING_DOCS,
                             cx.tcx.sess.source_map().def_span(macro_def.span),
                             "missing documentation for macro");
            }
        }
    }

    fn check_item(&mut self, cx: &LateContext<'_, '_>, it: &hir::Item) {
        let desc = match it.node {
            hir::ItemKind::Fn(..) => "a function",
            hir::ItemKind::Mod(..) => "a module",
            hir::ItemKind::Enum(..) => "an enum",
            hir::ItemKind::Struct(..) => "a struct",
            hir::ItemKind::Union(..) => "a union",
            hir::ItemKind::Trait(.., ref trait_item_refs) => {
                // Issue #11592, traits are always considered exported, even when private.
                if let hir::VisibilityKind::Inherited = it.vis.node {
                    self.private_traits.insert(it.hir_id);
                    for trait_item_ref in trait_item_refs {
                        self.private_traits.insert(trait_item_ref.id.hir_id);
                    }
                    return;
                }
                "a trait"
            }
            hir::ItemKind::Ty(..) => "a type alias",
            hir::ItemKind::Impl(.., Some(ref trait_ref), _, ref impl_item_refs) => {
                // If the trait is private, add the impl items to private_traits so they don't get
                // reported for missing docs.
                let real_trait = trait_ref.path.def.def_id();
                if let Some(hir_id) = cx.tcx.hir().as_local_hir_id(real_trait) {
                    match cx.tcx.hir().find_by_hir_id(hir_id) {
                        Some(Node::Item(item)) => {
                            if let hir::VisibilityKind::Inherited = item.vis.node {
                                for impl_item_ref in impl_item_refs {
                                    self.private_traits.insert(impl_item_ref.id.hir_id);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                return;
            }
            hir::ItemKind::Const(..) => "a constant",
            hir::ItemKind::Static(..) => "a static",
            _ => return,
        };

        self.check_missing_docs_attrs(cx, Some(it.hir_id), &it.attrs, it.span, desc);
    }

    fn check_trait_item(&mut self, cx: &LateContext<'_, '_>, trait_item: &hir::TraitItem) {
        if self.private_traits.contains(&trait_item.hir_id) {
            return;
        }

        let desc = match trait_item.node {
            hir::TraitItemKind::Const(..) => "an associated constant",
            hir::TraitItemKind::Method(..) => "a trait method",
            hir::TraitItemKind::Type(..) => "an associated type",
        };

        self.check_missing_docs_attrs(cx,
                                      Some(trait_item.hir_id),
                                      &trait_item.attrs,
                                      trait_item.span,
                                      desc);
    }

    fn check_impl_item(&mut self, cx: &LateContext<'_, '_>, impl_item: &hir::ImplItem) {
        // If the method is an impl for a trait, don't doc.
        if method_context(cx, impl_item.hir_id) == MethodLateContext::TraitImpl {
            return;
        }

        let desc = match impl_item.node {
            hir::ImplItemKind::Const(..) => "an associated constant",
            hir::ImplItemKind::Method(..) => "a method",
            hir::ImplItemKind::Type(_) => "an associated type",
            hir::ImplItemKind::Existential(_) => "an associated existential type",
        };
        self.check_missing_docs_attrs(cx,
                                      Some(impl_item.hir_id),
                                      &impl_item.attrs,
                                      impl_item.span,
                                      desc);
    }

    fn check_struct_field(&mut self, cx: &LateContext<'_, '_>, sf: &hir::StructField) {
        if !sf.is_positional() {
            self.check_missing_docs_attrs(cx,
                                          Some(sf.hir_id),
                                          &sf.attrs,
                                          sf.span,
                                          "a struct field")
        }
    }

    fn check_variant(&mut self, cx: &LateContext<'_, '_>, v: &hir::Variant, _: &hir::Generics) {
        self.check_missing_docs_attrs(cx,
                                      Some(v.node.id),
                                      &v.node.attrs,
                                      v.span,
                                      "a variant");
    }
}

declare_lint! {
    pub MISSING_COPY_IMPLEMENTATIONS,
    Allow,
    "detects potentially-forgotten implementations of `Copy`"
}

#[derive(Copy, Clone)]
pub struct MissingCopyImplementations;

impl LintPass for MissingCopyImplementations {
    fn name(&self) -> &'static str {
        "MissingCopyImplementations"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!(MISSING_COPY_IMPLEMENTATIONS)
    }
}

impl<'a, 'tcx> LateLintPass<'a, 'tcx> for MissingCopyImplementations {
    fn check_item(&mut self, cx: &LateContext<'_, '_>, item: &hir::Item) {
        if !cx.access_levels.is_reachable(item.hir_id) {
            return;
        }
        let (def, ty) = match item.node {
            hir::ItemKind::Struct(_, ref ast_generics) => {
                if !ast_generics.params.is_empty() {
                    return;
                }
                let def = cx.tcx.adt_def(cx.tcx.hir().local_def_id_from_hir_id(item.hir_id));
                (def, cx.tcx.mk_adt(def, cx.tcx.intern_substs(&[])))
            }
            hir::ItemKind::Union(_, ref ast_generics) => {
                if !ast_generics.params.is_empty() {
                    return;
                }
                let def = cx.tcx.adt_def(cx.tcx.hir().local_def_id_from_hir_id(item.hir_id));
                (def, cx.tcx.mk_adt(def, cx.tcx.intern_substs(&[])))
            }
            hir::ItemKind::Enum(_, ref ast_generics) => {
                if !ast_generics.params.is_empty() {
                    return;
                }
                let def = cx.tcx.adt_def(cx.tcx.hir().local_def_id_from_hir_id(item.hir_id));
                (def, cx.tcx.mk_adt(def, cx.tcx.intern_substs(&[])))
            }
            _ => return,
        };
        if def.has_dtor(cx.tcx) {
            return;
        }
        let param_env = ty::ParamEnv::empty();
        if ty.is_copy_modulo_regions(cx.tcx, param_env, item.span) {
            return;
        }
        if param_env.can_type_implement_copy(cx.tcx, ty).is_ok() {
            cx.span_lint(MISSING_COPY_IMPLEMENTATIONS,
                         item.span,
                         "type could implement `Copy`; consider adding `impl \
                          Copy`")
        }
    }
}

declare_lint! {
    MISSING_DEBUG_IMPLEMENTATIONS,
    Allow,
    "detects missing implementations of fmt::Debug"
}

pub struct MissingDebugImplementations {
    impling_types: Option<HirIdSet>,
}

impl MissingDebugImplementations {
    pub fn new() -> MissingDebugImplementations {
        MissingDebugImplementations { impling_types: None }
    }
}

impl LintPass for MissingDebugImplementations {
    fn name(&self) -> &'static str {
        "MissingDebugImplementations"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!(MISSING_DEBUG_IMPLEMENTATIONS)
    }
}

impl<'a, 'tcx> LateLintPass<'a, 'tcx> for MissingDebugImplementations {
    fn check_item(&mut self, cx: &LateContext<'_, '_>, item: &hir::Item) {
        if !cx.access_levels.is_reachable(item.hir_id) {
            return;
        }

        match item.node {
            hir::ItemKind::Struct(..) |
            hir::ItemKind::Union(..) |
            hir::ItemKind::Enum(..) => {}
            _ => return,
        }

        let debug = match cx.tcx.lang_items().debug_trait() {
            Some(debug) => debug,
            None => return,
        };

        if self.impling_types.is_none() {
            let mut impls = HirIdSet::default();
            cx.tcx.for_each_impl(debug, |d| {
                if let Some(ty_def) = cx.tcx.type_of(d).ty_adt_def() {
                    if let Some(hir_id) = cx.tcx.hir().as_local_hir_id(ty_def.did) {
                        impls.insert(hir_id);
                    }
                }
            });

            self.impling_types = Some(impls);
            debug!("{:?}", self.impling_types);
        }

        if !self.impling_types.as_ref().unwrap().contains(&item.hir_id) {
            cx.span_lint(MISSING_DEBUG_IMPLEMENTATIONS,
                         item.span,
                         "type does not implement `fmt::Debug`; consider adding #[derive(Debug)] \
                          or a manual implementation")
        }
    }
}

declare_lint! {
    pub ANONYMOUS_PARAMETERS,
    Allow,
    "detects anonymous parameters"
}

/// Checks for use of anonymous parameters (RFC 1685).
#[derive(Copy, Clone)]
pub struct AnonymousParameters;

impl LintPass for AnonymousParameters {
    fn name(&self) -> &'static str {
        "AnonymousParameters"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!(ANONYMOUS_PARAMETERS)
    }
}

impl EarlyLintPass for AnonymousParameters {
    fn check_trait_item(&mut self, cx: &EarlyContext<'_>, it: &ast::TraitItem) {
        match it.node {
            ast::TraitItemKind::Method(ref sig, _) => {
                for arg in sig.decl.inputs.iter() {
                    match arg.pat.node {
                        ast::PatKind::Ident(_, ident, None) => {
                            if ident.name == keywords::Invalid.name() {
                                let ty_snip = cx
                                    .sess
                                    .source_map()
                                    .span_to_snippet(arg.ty.span);

                                let (ty_snip, appl) = if let Ok(snip) = ty_snip {
                                    (snip, Applicability::MachineApplicable)
                                } else {
                                    ("<type>".to_owned(), Applicability::HasPlaceholders)
                                };

                                cx.struct_span_lint(
                                    ANONYMOUS_PARAMETERS,
                                    arg.pat.span,
                                    "anonymous parameters are deprecated and will be \
                                     removed in the next edition."
                                ).span_suggestion(
                                    arg.pat.span,
                                    "Try naming the parameter or explicitly \
                                    ignoring it",
                                    format!("_: {}", ty_snip),
                                    appl
                                ).emit();
                            }
                        }
                        _ => (),
                    }
                }
            },
            _ => (),
        }
    }
}

/// Check for use of attributes which have been deprecated.
#[derive(Clone)]
pub struct DeprecatedAttr {
    // This is not free to compute, so we want to keep it around, rather than
    // compute it for every attribute.
    depr_attrs: Vec<&'static (&'static str, AttributeType, AttributeTemplate, AttributeGate)>,
}

impl DeprecatedAttr {
    pub fn new() -> DeprecatedAttr {
        DeprecatedAttr {
            depr_attrs: deprecated_attributes(),
        }
    }
}

impl LintPass for DeprecatedAttr {
    fn name(&self) -> &'static str {
        "DeprecatedAttr"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!()
    }
}

impl EarlyLintPass for DeprecatedAttr {
    fn check_attribute(&mut self, cx: &EarlyContext<'_>, attr: &ast::Attribute) {
        let name = attr.name_or_empty();
        for &&(n, _, _, ref g) in &self.depr_attrs {
            if name == n {
                if let &AttributeGate::Gated(Stability::Deprecated(link, suggestion),
                                             ref name,
                                             ref reason,
                                             _) = g {
                    let msg = format!("use of deprecated attribute `{}`: {}. See {}",
                                      name, reason, link);
                    let mut err = cx.struct_span_lint(DEPRECATED, attr.span, &msg);
                    err.span_suggestion_short(
                        attr.span,
                        suggestion.unwrap_or("remove this attribute"),
                        String::new(),
                        Applicability::MachineApplicable
                    );
                    err.emit();
                }
                return;
            }
        }
    }
}

declare_lint! {
    pub UNUSED_DOC_COMMENTS,
    Warn,
    "detects doc comments that aren't used by rustdoc"
}

#[derive(Copy, Clone)]
pub struct UnusedDocComment;

impl LintPass for UnusedDocComment {
    fn name(&self) -> &'static str {
        "UnusedDocComment"
    }

    fn get_lints(&self) -> LintArray {
        lint_array![UNUSED_DOC_COMMENTS]
    }
}

impl UnusedDocComment {
    fn warn_if_doc(
        &self,
        cx: &EarlyContext<'_>,
        node_span: Span,
        node_kind: &str,
        is_macro_expansion: bool,
        attrs: &[ast::Attribute]
    ) {
        let mut attrs = attrs.into_iter().peekable();

        // Accumulate a single span for sugared doc comments.
        let mut sugared_span: Option<Span> = None;

        while let Some(attr) = attrs.next() {
            if attr.is_sugared_doc {
                sugared_span = Some(
                    sugared_span.map_or_else(
                        || attr.span,
                        |span| span.with_hi(attr.span.hi()),
                    ),
                );
            }

            if attrs.peek().map(|next_attr| next_attr.is_sugared_doc).unwrap_or_default() {
                continue;
            }

            let span = sugared_span.take().unwrap_or_else(|| attr.span);

            if attr.check_name("doc") {
                let mut err = cx.struct_span_lint(UNUSED_DOC_COMMENTS, span, "unused doc comment");

                err.span_label(
                    node_span,
                    format!("rustdoc does not generate documentation for {}", node_kind)
                );

                if is_macro_expansion {
                    err.help("to document an item produced by a macro, \
                              the macro must produce the documentation as part of its expansion");
                }

                err.emit();
            }
        }
    }
}

impl EarlyLintPass for UnusedDocComment {
    fn check_item(&mut self, cx: &EarlyContext<'_>, item: &ast::Item) {
        if let ast::ItemKind::Mac(..) = item.node {
            self.warn_if_doc(cx, item.span, "macro expansions", true, &item.attrs);
        }
    }

    fn check_stmt(&mut self, cx: &EarlyContext<'_>, stmt: &ast::Stmt) {
        let (kind, is_macro_expansion) = match stmt.node {
            ast::StmtKind::Local(..) => ("statements", false),
            ast::StmtKind::Item(..) => ("inner items", false),
            ast::StmtKind::Mac(..) => ("macro expansions", true),
            // expressions will be reported by `check_expr`.
            ast::StmtKind::Semi(..) |
            ast::StmtKind::Expr(..) => return,
        };

        self.warn_if_doc(cx, stmt.span, kind, is_macro_expansion, stmt.node.attrs());
    }

    fn check_arm(&mut self, cx: &EarlyContext<'_>, arm: &ast::Arm) {
        let arm_span = arm.pats[0].span.with_hi(arm.body.span.hi());
        self.warn_if_doc(cx, arm_span, "match arms", false, &arm.attrs);
    }

    fn check_expr(&mut self, cx: &EarlyContext<'_>, expr: &ast::Expr) {
        self.warn_if_doc(cx, expr.span, "expressions", false, &expr.attrs);
    }
}

declare_lint! {
    PLUGIN_AS_LIBRARY,
    Warn,
    "compiler plugin used as ordinary library in non-plugin crate"
}

#[derive(Copy, Clone)]
pub struct PluginAsLibrary;

impl LintPass for PluginAsLibrary {
    fn name(&self) -> &'static str {
        "PluginAsLibrary"
    }

    fn get_lints(&self) -> LintArray {
        lint_array![PLUGIN_AS_LIBRARY]
    }
}

impl<'a, 'tcx> LateLintPass<'a, 'tcx> for PluginAsLibrary {
    fn check_item(&mut self, cx: &LateContext<'_, '_>, it: &hir::Item) {
        if cx.tcx.plugin_registrar_fn(LOCAL_CRATE).is_some() {
            // We're compiling a plugin; it's fine to link other plugins.
            return;
        }

        match it.node {
            hir::ItemKind::ExternCrate(..) => (),
            _ => return,
        };

        let def_id = cx.tcx.hir().local_def_id_from_hir_id(it.hir_id);
        let prfn = match cx.tcx.extern_mod_stmt_cnum(def_id) {
            Some(cnum) => cx.tcx.plugin_registrar_fn(cnum),
            None => {
                // Probably means we aren't linking the crate for some reason.
                //
                // Not sure if / when this could happen.
                return;
            }
        };

        if prfn.is_some() {
            cx.span_lint(PLUGIN_AS_LIBRARY,
                         it.span,
                         "compiler plugin used as an ordinary library");
        }
    }
}

declare_lint! {
    NO_MANGLE_CONST_ITEMS,
    Deny,
    "const items will not have their symbols exported"
}

declare_lint! {
    NO_MANGLE_GENERIC_ITEMS,
    Warn,
    "generic items must be mangled"
}

#[derive(Copy, Clone)]
pub struct InvalidNoMangleItems;

impl LintPass for InvalidNoMangleItems {
    fn name(&self) -> &'static str {
        "InvalidNoMangleItems"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!(NO_MANGLE_CONST_ITEMS,
                    NO_MANGLE_GENERIC_ITEMS)
    }
}

impl<'a, 'tcx> LateLintPass<'a, 'tcx> for InvalidNoMangleItems {
    fn check_item(&mut self, cx: &LateContext<'_, '_>, it: &hir::Item) {
        match it.node {
            hir::ItemKind::Fn(.., ref generics, _) => {
                if let Some(no_mangle_attr) = attr::find_by_name(&it.attrs, "no_mangle") {
                    for param in &generics.params {
                        match param.kind {
                            GenericParamKind::Lifetime { .. } => {}
                            GenericParamKind::Type { .. } |
                            GenericParamKind::Const { .. } => {
                                let mut err = cx.struct_span_lint(
                                    NO_MANGLE_GENERIC_ITEMS,
                                    it.span,
                                    "functions generic over types or consts must be mangled",
                                );
                                err.span_suggestion_short(
                                    no_mangle_attr.span,
                                    "remove this attribute",
                                    String::new(),
                                    // Use of `#[no_mangle]` suggests FFI intent; correct
                                    // fix may be to monomorphize source by hand
                                    Applicability::MaybeIncorrect
                                );
                                err.emit();
                                break;
                            }
                        }
                    }
                }
            }
            hir::ItemKind::Const(..) => {
                if attr::contains_name(&it.attrs, "no_mangle") {
                    // Const items do not refer to a particular location in memory, and therefore
                    // don't have anything to attach a symbol to
                    let msg = "const items should never be #[no_mangle]";
                    let mut err = cx.struct_span_lint(NO_MANGLE_CONST_ITEMS, it.span, msg);

                    // account for "pub const" (#45562)
                    let start = cx.tcx.sess.source_map().span_to_snippet(it.span)
                        .map(|snippet| snippet.find("const").unwrap_or(0))
                        .unwrap_or(0) as u32;
                    // `const` is 5 chars
                    let const_span = it.span.with_hi(BytePos(it.span.lo().0 + start + 5));
                    err.span_suggestion(
                        const_span,
                        "try a static value",
                        "pub static".to_owned(),
                        Applicability::MachineApplicable
                    );
                    err.emit();
                }
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy)]
pub struct MutableTransmutes;

declare_lint! {
    MUTABLE_TRANSMUTES,
    Deny,
    "mutating transmuted &mut T from &T may cause undefined behavior"
}

impl LintPass for MutableTransmutes {
    fn name(&self) -> &'static str {
        "MutableTransmutes"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!(MUTABLE_TRANSMUTES)
    }
}

impl<'a, 'tcx> LateLintPass<'a, 'tcx> for MutableTransmutes {
    fn check_expr(&mut self, cx: &LateContext<'_, '_>, expr: &hir::Expr) {
        use rustc_target::spec::abi::Abi::RustIntrinsic;

        let msg = "mutating transmuted &mut T from &T may cause undefined behavior, \
                   consider instead using an UnsafeCell";
        match get_transmute_from_to(cx, expr) {
            Some((&ty::Ref(_, _, from_mt), &ty::Ref(_, _, to_mt))) => {
                if to_mt == hir::Mutability::MutMutable &&
                   from_mt == hir::Mutability::MutImmutable {
                    cx.span_lint(MUTABLE_TRANSMUTES, expr.span, msg);
                }
            }
            _ => (),
        }

        fn get_transmute_from_to<'a, 'tcx>
            (cx: &LateContext<'a, 'tcx>,
             expr: &hir::Expr)
             -> Option<(&'tcx ty::TyKind<'tcx>, &'tcx ty::TyKind<'tcx>)> {
            let def = if let hir::ExprKind::Path(ref qpath) = expr.node {
                cx.tables.qpath_def(qpath, expr.hir_id)
            } else {
                return None;
            };
            if let Def::Fn(did) = def {
                if !def_id_is_transmute(cx, did) {
                    return None;
                }
                let sig = cx.tables.node_type(expr.hir_id).fn_sig(cx.tcx);
                let from = sig.inputs().skip_binder()[0];
                let to = *sig.output().skip_binder();
                return Some((&from.sty, &to.sty));
            }
            None
        }

        fn def_id_is_transmute(cx: &LateContext<'_, '_>, def_id: DefId) -> bool {
            cx.tcx.fn_sig(def_id).abi() == RustIntrinsic &&
            cx.tcx.item_name(def_id) == "transmute"
        }
    }
}

/// Forbids using the `#[feature(...)]` attribute
#[derive(Copy, Clone)]
pub struct UnstableFeatures;

declare_lint! {
    UNSTABLE_FEATURES,
    Allow,
    "enabling unstable features (deprecated. do not use)"
}

impl LintPass for UnstableFeatures {
    fn name(&self) -> &'static str {
        "UnstableFeatures"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!(UNSTABLE_FEATURES)
    }
}

impl<'a, 'tcx> LateLintPass<'a, 'tcx> for UnstableFeatures {
    fn check_attribute(&mut self, ctx: &LateContext<'_, '_>, attr: &ast::Attribute) {
        if attr.check_name("feature") {
            if let Some(items) = attr.meta_item_list() {
                for item in items {
                    ctx.span_lint(UNSTABLE_FEATURES, item.span(), "unstable feature");
                }
            }
        }
    }
}

/// Lint for unions that contain fields with possibly non-trivial destructors.
pub struct UnionsWithDropFields;

declare_lint! {
    UNIONS_WITH_DROP_FIELDS,
    Warn,
    "use of unions that contain fields with possibly non-trivial drop code"
}

impl LintPass for UnionsWithDropFields {
    fn name(&self) -> &'static str {
        "UnionsWithDropFields"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!(UNIONS_WITH_DROP_FIELDS)
    }
}

impl<'a, 'tcx> LateLintPass<'a, 'tcx> for UnionsWithDropFields {
    fn check_item(&mut self, ctx: &LateContext<'_, '_>, item: &hir::Item) {
        if let hir::ItemKind::Union(ref vdata, _) = item.node {
            for field in vdata.fields() {
                let field_ty = ctx.tcx.type_of(
                    ctx.tcx.hir().local_def_id_from_hir_id(field.hir_id));
                if field_ty.needs_drop(ctx.tcx, ctx.param_env) {
                    ctx.span_lint(UNIONS_WITH_DROP_FIELDS,
                                  field.span,
                                  "union contains a field with possibly non-trivial drop code, \
                                   drop code of union fields is ignored when dropping the union");
                    return;
                }
            }
        }
    }
}

/// Lint for items marked `pub` that aren't reachable from other crates.
#[derive(Copy, Clone)]
pub struct UnreachablePub;

declare_lint! {
    pub UNREACHABLE_PUB,
    Allow,
    "`pub` items not reachable from crate root"
}

impl LintPass for UnreachablePub {
    fn name(&self) -> &'static str {
        "UnreachablePub"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!(UNREACHABLE_PUB)
    }
}

impl UnreachablePub {
    fn perform_lint(&self, cx: &LateContext<'_, '_>, what: &str, id: hir::HirId,
                    vis: &hir::Visibility, span: Span, exportable: bool) {
        let mut applicability = Applicability::MachineApplicable;
        match vis.node {
            hir::VisibilityKind::Public if !cx.access_levels.is_reachable(id) => {
                if span.ctxt().outer().expn_info().is_some() {
                    applicability = Applicability::MaybeIncorrect;
                }
                let def_span = cx.tcx.sess.source_map().def_span(span);
                let mut err = cx.struct_span_lint(UNREACHABLE_PUB, def_span,
                                                  &format!("unreachable `pub` {}", what));
                let replacement = if cx.tcx.features().crate_visibility_modifier {
                    "crate"
                } else {
                    "pub(crate)"
                }.to_owned();

                err.span_suggestion(
                    vis.span,
                    "consider restricting its visibility",
                    replacement,
                    applicability,
                );
                if exportable {
                    err.help("or consider exporting it for use by other crates");
                }
                err.emit();
            },
            _ => {}
        }
    }
}


impl<'a, 'tcx> LateLintPass<'a, 'tcx> for UnreachablePub {
    fn check_item(&mut self, cx: &LateContext<'_, '_>, item: &hir::Item) {
        self.perform_lint(cx, "item", item.hir_id, &item.vis, item.span, true);
    }

    fn check_foreign_item(&mut self, cx: &LateContext<'_, '_>, foreign_item: &hir::ForeignItem) {
        self.perform_lint(cx, "item", foreign_item.hir_id, &foreign_item.vis,
                          foreign_item.span, true);
    }

    fn check_struct_field(&mut self, cx: &LateContext<'_, '_>, field: &hir::StructField) {
        self.perform_lint(cx, "field", field.hir_id, &field.vis, field.span, false);
    }

    fn check_impl_item(&mut self, cx: &LateContext<'_, '_>, impl_item: &hir::ImplItem) {
        self.perform_lint(cx, "item", impl_item.hir_id, &impl_item.vis, impl_item.span, false);
    }
}

/// Lint for trait and lifetime bounds in type aliases being mostly ignored.
/// They are relevant when using associated types, but otherwise neither checked
/// at definition site nor enforced at use site.

pub struct TypeAliasBounds;

declare_lint! {
    TYPE_ALIAS_BOUNDS,
    Warn,
    "bounds in type aliases are not enforced"
}

impl LintPass for TypeAliasBounds {
    fn name(&self) -> &'static str {
        "TypeAliasBounds"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!(TYPE_ALIAS_BOUNDS)
    }
}

impl TypeAliasBounds {
    fn is_type_variable_assoc(qpath: &hir::QPath) -> bool {
        match *qpath {
            hir::QPath::TypeRelative(ref ty, _) => {
                // If this is a type variable, we found a `T::Assoc`.
                match ty.node {
                    hir::TyKind::Path(hir::QPath::Resolved(None, ref path)) => {
                        match path.def {
                            Def::TyParam(_) => true,
                            _ => false
                        }
                    }
                    _ => false
                }
            }
            hir::QPath::Resolved(..) => false,
        }
    }

    fn suggest_changing_assoc_types(ty: &hir::Ty, err: &mut DiagnosticBuilder<'_>) {
        // Access to associates types should use `<T as Bound>::Assoc`, which does not need a
        // bound.  Let's see if this type does that.

        // We use a HIR visitor to walk the type.
        use rustc::hir::intravisit::{self, Visitor};
        struct WalkAssocTypes<'a, 'db> where 'db: 'a {
            err: &'a mut DiagnosticBuilder<'db>
        }
        impl<'a, 'db, 'v> Visitor<'v> for WalkAssocTypes<'a, 'db> {
            fn nested_visit_map<'this>(&'this mut self) -> intravisit::NestedVisitorMap<'this, 'v>
            {
                intravisit::NestedVisitorMap::None
            }

            fn visit_qpath(&mut self, qpath: &'v hir::QPath, id: hir::HirId, span: Span) {
                if TypeAliasBounds::is_type_variable_assoc(qpath) {
                    self.err.span_help(span,
                        "use fully disambiguated paths (i.e., `<T as Trait>::Assoc`) to refer to \
                         associated types in type aliases");
                }
                intravisit::walk_qpath(self, qpath, id, span)
            }
        }

        // Let's go for a walk!
        let mut visitor = WalkAssocTypes { err };
        visitor.visit_ty(ty);
    }
}

impl<'a, 'tcx> LateLintPass<'a, 'tcx> for TypeAliasBounds {
    fn check_item(&mut self, cx: &LateContext<'_, '_>, item: &hir::Item) {
        let (ty, type_alias_generics) = match item.node {
            hir::ItemKind::Ty(ref ty, ref generics) => (&*ty, generics),
            _ => return,
        };
        let mut suggested_changing_assoc_types = false;
        // There must not be a where clause
        if !type_alias_generics.where_clause.predicates.is_empty() {
            let spans : Vec<_> = type_alias_generics.where_clause.predicates.iter()
                .map(|pred| pred.span()).collect();
            let mut err = cx.struct_span_lint(TYPE_ALIAS_BOUNDS, spans,
                "where clauses are not enforced in type aliases");
            err.help("the clause will not be checked when the type alias is used, \
                      and should be removed");
            if !suggested_changing_assoc_types {
                TypeAliasBounds::suggest_changing_assoc_types(ty, &mut err);
                suggested_changing_assoc_types = true;
            }
            err.emit();
        }
        // The parameters must not have bounds
        for param in type_alias_generics.params.iter() {
            let spans: Vec<_> = param.bounds.iter().map(|b| b.span()).collect();
            if !spans.is_empty() {
                let mut err = cx.struct_span_lint(
                    TYPE_ALIAS_BOUNDS,
                    spans,
                    "bounds on generic parameters are not enforced in type aliases",
                );
                err.help("the bound will not be checked when the type alias is used, \
                          and should be removed");
                if !suggested_changing_assoc_types {
                    TypeAliasBounds::suggest_changing_assoc_types(ty, &mut err);
                    suggested_changing_assoc_types = true;
                }
                err.emit();
            }
        }
    }
}

/// Lint constants that are erroneous.
/// Without this lint, we might not get any diagnostic if the constant is
/// unused within this crate, even though downstream crates can't use it
/// without producing an error.
pub struct UnusedBrokenConst;

impl LintPass for UnusedBrokenConst {
    fn name(&self) -> &'static str {
        "UnusedBrokenConst"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!()
    }
}
fn check_const(cx: &LateContext<'_, '_>, body_id: hir::BodyId) {
    let def_id = cx.tcx.hir().body_owner_def_id(body_id);
    let is_static = cx.tcx.is_static(def_id).is_some();
    let param_env = if is_static {
        // Use the same param_env as `codegen_static_initializer`, to reuse the cache.
        ty::ParamEnv::reveal_all()
    } else {
        cx.tcx.param_env(def_id)
    };
    let cid = ::rustc::mir::interpret::GlobalId {
        instance: ty::Instance::mono(cx.tcx, def_id),
        promoted: None
    };
    // trigger the query once for all constants since that will already report the errors
    // FIXME: Use ensure here
    let _ = cx.tcx.const_eval(param_env.and(cid));
}

impl<'a, 'tcx> LateLintPass<'a, 'tcx> for UnusedBrokenConst {
    fn check_item(&mut self, cx: &LateContext<'_, '_>, it: &hir::Item) {
        match it.node {
            hir::ItemKind::Const(_, body_id) => {
                check_const(cx, body_id);
            },
            hir::ItemKind::Static(_, _, body_id) => {
                check_const(cx, body_id);
            },
            _ => {},
        }
    }
}

/// Lint for trait and lifetime bounds that don't depend on type parameters
/// which either do nothing, or stop the item from being used.
pub struct TrivialConstraints;

declare_lint! {
    TRIVIAL_BOUNDS,
    Warn,
    "these bounds don't depend on an type parameters"
}

impl LintPass for TrivialConstraints {
    fn name(&self) -> &'static str {
        "TrivialConstraints"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!(TRIVIAL_BOUNDS)
    }
}

impl<'a, 'tcx> LateLintPass<'a, 'tcx> for TrivialConstraints {
    fn check_item(
        &mut self,
        cx: &LateContext<'a, 'tcx>,
        item: &'tcx hir::Item,
    ) {
        use rustc::ty::fold::TypeFoldable;
        use rustc::ty::Predicate::*;


        if cx.tcx.features().trivial_bounds {
            let def_id = cx.tcx.hir().local_def_id_from_hir_id(item.hir_id);
            let predicates = cx.tcx.predicates_of(def_id);
            for &(predicate, span) in &predicates.predicates {
                let predicate_kind_name = match predicate {
                    Trait(..) => "Trait",
                    TypeOutlives(..) |
                    RegionOutlives(..) => "Lifetime",

                    // Ignore projections, as they can only be global
                    // if the trait bound is global
                    Projection(..) |
                    // Ignore bounds that a user can't type
                    WellFormed(..) |
                    ObjectSafe(..) |
                    ClosureKind(..) |
                    Subtype(..) |
                    ConstEvaluatable(..) => continue,
                };
                if predicate.is_global() {
                    cx.span_lint(
                        TRIVIAL_BOUNDS,
                        span,
                        &format!("{} bound {} does not depend on any type \
                                or lifetime parameters", predicate_kind_name, predicate),
                    );
                }
            }
        }
    }
}


/// Does nothing as a lint pass, but registers some `Lint`s
/// which are used by other parts of the compiler.
#[derive(Copy, Clone)]
pub struct SoftLints;

impl LintPass for SoftLints {
    fn name(&self) -> &'static str {
        "SoftLints"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!(
            WHILE_TRUE,
            BOX_POINTERS,
            NON_SHORTHAND_FIELD_PATTERNS,
            UNSAFE_CODE,
            MISSING_DOCS,
            MISSING_COPY_IMPLEMENTATIONS,
            MISSING_DEBUG_IMPLEMENTATIONS,
            ANONYMOUS_PARAMETERS,
            UNUSED_DOC_COMMENTS,
            PLUGIN_AS_LIBRARY,
            NO_MANGLE_CONST_ITEMS,
            NO_MANGLE_GENERIC_ITEMS,
            MUTABLE_TRANSMUTES,
            UNSTABLE_FEATURES,
            UNIONS_WITH_DROP_FIELDS,
            UNREACHABLE_PUB,
            TYPE_ALIAS_BOUNDS,
            TRIVIAL_BOUNDS
        )
    }
}

declare_lint! {
    pub ELLIPSIS_INCLUSIVE_RANGE_PATTERNS,
    Allow,
    "`...` range patterns are deprecated"
}


pub struct EllipsisInclusiveRangePatterns;

impl LintPass for EllipsisInclusiveRangePatterns {
    fn name(&self) -> &'static str {
        "EllipsisInclusiveRangePatterns"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!(ELLIPSIS_INCLUSIVE_RANGE_PATTERNS)
    }
}

impl EarlyLintPass for EllipsisInclusiveRangePatterns {
    fn check_pat(&mut self, cx: &EarlyContext<'_>, pat: &ast::Pat, visit_subpats: &mut bool) {
        use self::ast::{PatKind, RangeEnd, RangeSyntax::DotDotDot};

        /// If `pat` is a `...` pattern, return the start and end of the range, as well as the span
        /// corresponding to the ellipsis.
        fn matches_ellipsis_pat(pat: &ast::Pat) -> Option<(&P<Expr>, &P<Expr>, Span)> {
            match &pat.node {
                PatKind::Range(a, b, Spanned { span, node: RangeEnd::Included(DotDotDot), .. }) => {
                    Some((a, b, *span))
                }
                _ => None,
            }
        }

        let (parenthesise, endpoints) = match &pat.node {
            PatKind::Ref(subpat, _) => (true, matches_ellipsis_pat(&subpat)),
            _ => (false, matches_ellipsis_pat(pat)),
        };

        if let Some((start, end, join)) = endpoints {
            let msg = "`...` range patterns are deprecated";
            let suggestion = "use `..=` for an inclusive range";
            if parenthesise {
                *visit_subpats = false;
                let mut err = cx.struct_span_lint(ELLIPSIS_INCLUSIVE_RANGE_PATTERNS, pat.span, msg);
                err.span_suggestion(
                    pat.span,
                    suggestion,
                    format!("&({}..={})", expr_to_string(&start), expr_to_string(&end)),
                    Applicability::MachineApplicable,
                );
                err.emit();
            } else {
                let mut err = cx.struct_span_lint(ELLIPSIS_INCLUSIVE_RANGE_PATTERNS, join, msg);
                err.span_suggestion_short(
                    join,
                    suggestion,
                    "..=".to_owned(),
                    Applicability::MachineApplicable,
                );
                err.emit();
            };
        }
    }
}

declare_lint! {
    UNNAMEABLE_TEST_ITEMS,
    Warn,
    "detects an item that cannot be named being marked as #[test_case]",
    report_in_external_macro: true
}

pub struct UnnameableTestItems {
    boundary: hir::HirId, // HirId of the item under which things are not nameable
    items_nameable: bool,
}

impl UnnameableTestItems {
    pub fn new() -> Self {
        Self {
            boundary: hir::DUMMY_HIR_ID,
            items_nameable: true
        }
    }
}

impl LintPass for UnnameableTestItems {
    fn name(&self) -> &'static str {
        "UnnameableTestItems"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!(UNNAMEABLE_TEST_ITEMS)
    }
}

impl<'a, 'tcx> LateLintPass<'a, 'tcx> for UnnameableTestItems {
    fn check_item(&mut self, cx: &LateContext<'_, '_>, it: &hir::Item) {
        if self.items_nameable {
            if let hir::ItemKind::Mod(..) = it.node {}
            else {
                self.items_nameable = false;
                self.boundary = it.hir_id;
            }
            return;
        }

        if let Some(attr) = attr::find_by_name(&it.attrs, "rustc_test_marker") {
            cx.struct_span_lint(
                UNNAMEABLE_TEST_ITEMS,
                attr.span,
                "cannot test inner items",
            ).emit();
        }
    }

    fn check_item_post(&mut self, _cx: &LateContext<'_, '_>, it: &hir::Item) {
        if !self.items_nameable && self.boundary == it.hir_id {
            self.items_nameable = true;
        }
    }
}

declare_lint! {
    pub KEYWORD_IDENTS,
    Allow,
    "detects edition keywords being used as an identifier"
}

/// Check for uses of edition keywords used as an identifier.
#[derive(Copy, Clone)]
pub struct KeywordIdents;

impl LintPass for KeywordIdents {
    fn name(&self) -> &'static str {
        "KeywordIdents"
    }

    fn get_lints(&self) -> LintArray {
        lint_array!(KEYWORD_IDENTS)
    }
}

impl KeywordIdents {
    fn check_tokens(&mut self, cx: &EarlyContext<'_>, tokens: TokenStream) {
        for tt in tokens.into_trees() {
            match tt {
                TokenTree::Token(span, tok) => match tok.ident() {
                    // only report non-raw idents
                    Some((ident, false)) => {
                        self.check_ident(cx, ast::Ident {
                            span: span.substitute_dummy(ident.span),
                            ..ident
                        });
                    }
                    _ => {},
                }
                TokenTree::Delimited(_, _, tts) => {
                    self.check_tokens(cx, tts)
                },
            }
        }
    }
}

impl EarlyLintPass for KeywordIdents {
    fn check_mac_def(&mut self, cx: &EarlyContext<'_>, mac_def: &ast::MacroDef, _id: ast::NodeId) {
        self.check_tokens(cx, mac_def.stream());
    }
    fn check_mac(&mut self, cx: &EarlyContext<'_>, mac: &ast::Mac) {
        self.check_tokens(cx, mac.node.tts.clone().into());
    }
    fn check_ident(&mut self, cx: &EarlyContext<'_>, ident: ast::Ident) {
        let ident_str = &ident.as_str()[..];
        let cur_edition = cx.sess.edition();
        let is_raw_ident = |ident: ast::Ident| {
            cx.sess.parse_sess.raw_identifier_spans.borrow().contains(&ident.span)
        };
        let next_edition = match cur_edition {
            Edition::Edition2015 => {
                match ident_str {
                    "async" | "try" | "dyn" => Edition::Edition2018,
                    // Only issue warnings for `await` if the `async_await`
                    // feature isn't being used. Otherwise, users need
                    // to keep using `await` for the macro exposed by std.
                    "await" if !cx.sess.features_untracked().async_await => Edition::Edition2018,
                    _ => return,
                }
            }

            // There are no new keywords yet for the 2018 edition and beyond.
            // However, `await` is a "false" keyword in the 2018 edition,
            // and can only be used if the `async_await` feature is enabled.
            // Otherwise, we emit an error.
            _ => {
                if "await" == ident_str
                    && !cx.sess.features_untracked().async_await
                    && !is_raw_ident(ident)
                {
                    let mut err = struct_span_err!(
                        cx.sess,
                        ident.span,
                        E0721,
                        "`await` is a keyword in the {} edition", cur_edition,
                    );
                    err.span_suggestion(
                        ident.span,
                        "you can use a raw identifier to stay compatible",
                        "r#await".to_string(),
                        Applicability::MachineApplicable,
                    );
                    err.emit();
                }
                return
            },
        };

        // don't lint `r#foo`
        if is_raw_ident(ident) {
            return;
        }

        let mut lint = cx.struct_span_lint(
            KEYWORD_IDENTS,
            ident.span,
            &format!("`{}` is a keyword in the {} edition",
                     ident.as_str(),
                     next_edition),
        );
        lint.span_suggestion(
            ident.span,
            "you can use a raw identifier to stay compatible",
            format!("r#{}", ident.as_str()),
            Applicability::MachineApplicable,
        );
        lint.emit()
    }
}


pub struct ExplicitOutlivesRequirements;

impl LintPass for ExplicitOutlivesRequirements {
    fn name(&self) -> &'static str {
        "ExplicitOutlivesRequirements"
    }

    fn get_lints(&self) -> LintArray {
        lint_array![EXPLICIT_OUTLIVES_REQUIREMENTS]
    }
}

impl ExplicitOutlivesRequirements {
    fn collect_outlives_bound_spans(
        &self,
        cx: &LateContext<'_, '_>,
        item_def_id: DefId,
        param_name: &str,
        bounds: &hir::GenericBounds,
        infer_static: bool
    ) -> Vec<(usize, Span)> {
        // For lack of a more elegant strategy for comparing the `ty::Predicate`s
        // returned by this query with the params/bounds grabbed from the HIR—and
        // with some regrets—we're going to covert the param/lifetime names to
        // strings
        let inferred_outlives = cx.tcx.inferred_outlives_of(item_def_id);

        let ty_lt_names = inferred_outlives.iter().filter_map(|pred| {
            let binder = match pred {
                ty::Predicate::TypeOutlives(binder) => binder,
                _ => { return None; }
            };
            let ty_outlives_pred = binder.skip_binder();
            let ty_name = match ty_outlives_pred.0.sty {
                ty::Param(param) => param.name.to_string(),
                _ => { return None; }
            };
            let lt_name = match ty_outlives_pred.1 {
                ty::RegionKind::ReEarlyBound(region) => {
                    region.name.to_string()
                },
                _ => { return None; }
            };
            Some((ty_name, lt_name))
        }).collect::<Vec<_>>();

        let mut bound_spans = Vec::new();
        for (i, bound) in bounds.iter().enumerate() {
            if let hir::GenericBound::Outlives(lifetime) = bound {
                let is_static = match lifetime.name {
                    hir::LifetimeName::Static => true,
                    _ => false
                };
                if is_static && !infer_static {
                    // infer-outlives for 'static is still feature-gated (tracking issue #44493)
                    continue;
                }

                let lt_name = &lifetime.name.ident().to_string();
                if ty_lt_names.contains(&(param_name.to_owned(), lt_name.to_owned())) {
                    bound_spans.push((i, bound.span()));
                }
            }
        }
        bound_spans
    }

    fn consolidate_outlives_bound_spans(
        &self,
        lo: Span,
        bounds: &hir::GenericBounds,
        bound_spans: Vec<(usize, Span)>
    ) -> Vec<Span> {
        if bounds.is_empty() {
            return Vec::new();
        }
        if bound_spans.len() == bounds.len() {
            let (_, last_bound_span) = bound_spans[bound_spans.len()-1];
            // If all bounds are inferable, we want to delete the colon, so
            // start from just after the parameter (span passed as argument)
            vec![lo.to(last_bound_span)]
        } else {
            let mut merged = Vec::new();
            let mut last_merged_i = None;

            let mut from_start = true;
            for (i, bound_span) in bound_spans {
                match last_merged_i {
                    // If the first bound is inferable, our span should also eat the trailing `+`
                    None if i == 0 => {
                        merged.push(bound_span.to(bounds[1].span().shrink_to_lo()));
                        last_merged_i = Some(0);
                    },
                    // If consecutive bounds are inferable, merge their spans
                    Some(h) if i == h+1 => {
                        if let Some(tail) = merged.last_mut() {
                            // Also eat the trailing `+` if the first
                            // more-than-one bound is inferable
                            let to_span = if from_start && i < bounds.len() {
                                bounds[i+1].span().shrink_to_lo()
                            } else {
                                bound_span
                            };
                            *tail = tail.to(to_span);
                            last_merged_i = Some(i);
                        } else {
                            bug!("another bound-span visited earlier");
                        }
                    },
                    _ => {
                        // When we find a non-inferable bound, subsequent inferable bounds
                        // won't be consecutive from the start (and we'll eat the leading
                        // `+` rather than the trailing one)
                        from_start = false;
                        merged.push(bounds[i-1].span().shrink_to_hi().to(bound_span));
                        last_merged_i = Some(i);
                    }
                }
            }
            merged
        }
    }
}

impl<'a, 'tcx> LateLintPass<'a, 'tcx> for ExplicitOutlivesRequirements {
    fn check_item(&mut self, cx: &LateContext<'a, 'tcx>, item: &'tcx hir::Item) {
        let infer_static = cx.tcx.features().infer_static_outlives_requirements;
        let def_id = cx.tcx.hir().local_def_id_from_hir_id(item.hir_id);
        if let hir::ItemKind::Struct(_, ref generics) = item.node {
            let mut bound_count = 0;
            let mut lint_spans = Vec::new();

            for param in &generics.params {
                let param_name = match param.kind {
                    hir::GenericParamKind::Lifetime { .. } => continue,
                    hir::GenericParamKind::Type { .. } => {
                        match param.name {
                            hir::ParamName::Fresh(_) => continue,
                            hir::ParamName::Error => continue,
                            hir::ParamName::Plain(name) => name.to_string(),
                        }
                    }
                    hir::GenericParamKind::Const { .. } => continue,
                };
                let bound_spans = self.collect_outlives_bound_spans(
                    cx, def_id, &param_name, &param.bounds, infer_static
                );
                bound_count += bound_spans.len();
                lint_spans.extend(
                    self.consolidate_outlives_bound_spans(
                        param.span.shrink_to_hi(), &param.bounds, bound_spans
                    )
                );
            }

            let mut where_lint_spans = Vec::new();
            let mut dropped_predicate_count = 0;
            let num_predicates = generics.where_clause.predicates.len();
            for (i, where_predicate) in generics.where_clause.predicates.iter().enumerate() {
                if let hir::WherePredicate::BoundPredicate(predicate) = where_predicate {
                    let param_name = match predicate.bounded_ty.node {
                        hir::TyKind::Path(ref qpath) => {
                            if let hir::QPath::Resolved(None, ty_param_path) = qpath {
                                ty_param_path.segments[0].ident.to_string()
                            } else {
                                continue;
                            }
                        },
                        _ => { continue; }
                    };
                    let bound_spans = self.collect_outlives_bound_spans(
                        cx, def_id, &param_name, &predicate.bounds, infer_static
                    );
                    bound_count += bound_spans.len();

                    let drop_predicate = bound_spans.len() == predicate.bounds.len();
                    if drop_predicate {
                        dropped_predicate_count += 1;
                    }

                    // If all the bounds on a predicate were inferable and there are
                    // further predicates, we want to eat the trailing comma
                    if drop_predicate && i + 1 < num_predicates {
                        let next_predicate_span = generics.where_clause.predicates[i+1].span();
                        where_lint_spans.push(
                            predicate.span.to(next_predicate_span.shrink_to_lo())
                        );
                    } else {
                        where_lint_spans.extend(
                            self.consolidate_outlives_bound_spans(
                                predicate.span.shrink_to_lo(),
                                &predicate.bounds,
                                bound_spans
                            )
                        );
                    }
                }
            }

            // If all predicates are inferable, drop the entire clause
            // (including the `where`)
            if num_predicates > 0 && dropped_predicate_count == num_predicates {
                let full_where_span = generics.span.shrink_to_hi()
                    .to(generics.where_clause.span()
                    .expect("span of (nonempty) where clause should exist"));
                lint_spans.push(
                    full_where_span
                );
            } else {
                lint_spans.extend(where_lint_spans);
            }

            if !lint_spans.is_empty() {
                let mut err = cx.struct_span_lint(
                    EXPLICIT_OUTLIVES_REQUIREMENTS,
                    lint_spans.clone(),
                    "outlives requirements can be inferred"
                );
                err.multipart_suggestion(
                    if bound_count == 1 {
                        "remove this bound"
                    } else {
                        "remove these bounds"
                    },
                    lint_spans.into_iter().map(|span| (span, "".to_owned())).collect::<Vec<_>>(),
                    Applicability::MachineApplicable
                );
                err.emit();
            }

        }
    }

}
