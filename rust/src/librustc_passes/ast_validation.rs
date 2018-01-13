// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Validate AST before lowering it to HIR
//
// This pass is supposed to catch things that fit into AST data structures,
// but not permitted by the language. It runs after expansion when AST is frozen,
// so it can check for erroneous constructions produced by syntax extensions.
// This pass is supposed to perform only simple checks not requiring name resolution
// or type checking or some other kind of complex analysis.

use rustc::lint;
use rustc::session::Session;
use syntax::ast::*;
use syntax::attr;
use syntax::codemap::Spanned;
use syntax::parse::token;
use syntax::symbol::keywords;
use syntax::visit::{self, Visitor};
use syntax_pos::Span;
use errors;

struct AstValidator<'a> {
    session: &'a Session,
}

impl<'a> AstValidator<'a> {
    fn err_handler(&self) -> &errors::Handler {
        &self.session.parse_sess.span_diagnostic
    }

    fn check_lifetime(&self, lifetime: &Lifetime) {
        let valid_names = [keywords::StaticLifetime.name(), keywords::Invalid.name()];
        if !valid_names.contains(&lifetime.ident.name) &&
            token::Ident(lifetime.ident.without_first_quote()).is_reserved_ident() {
            self.err_handler().span_err(lifetime.span, "lifetimes cannot use keyword names");
        }
    }

    fn check_label(&self, label: Ident, span: Span) {
        if token::Ident(label.without_first_quote()).is_reserved_ident() || label.name == "'_" {
            self.err_handler().span_err(span, &format!("invalid label name `{}`", label.name));
        }
    }

    fn invalid_non_exhaustive_attribute(&self, variant: &Variant) {
        let has_non_exhaustive = attr::contains_name(&variant.node.attrs, "non_exhaustive");
        if has_non_exhaustive {
            self.err_handler().span_err(variant.span,
                                        "#[non_exhaustive] is not yet supported on variants");
        }
    }

    fn invalid_visibility(&self, vis: &Visibility, span: Span, note: Option<&str>) {
        if vis != &Visibility::Inherited {
            let mut err = struct_span_err!(self.session,
                                           span,
                                           E0449,
                                           "unnecessary visibility qualifier");
            if vis == &Visibility::Public {
                err.span_label(span, "`pub` not needed here");
            }
            if let Some(note) = note {
                err.note(note);
            }
            err.emit();
        }
    }

    fn check_decl_no_pat<ReportFn: Fn(Span, bool)>(&self, decl: &FnDecl, report_err: ReportFn) {
        for arg in &decl.inputs {
            match arg.pat.node {
                PatKind::Ident(BindingMode::ByValue(Mutability::Immutable), _, None) |
                PatKind::Wild => {}
                PatKind::Ident(BindingMode::ByValue(Mutability::Mutable), _, None) =>
                    report_err(arg.pat.span, true),
                _ => report_err(arg.pat.span, false),
            }
        }
    }

    fn check_trait_fn_not_const(&self, constness: Spanned<Constness>) {
        match constness.node {
            Constness::Const => {
                struct_span_err!(self.session, constness.span, E0379,
                                 "trait fns cannot be declared const")
                    .span_label(constness.span, "trait fns cannot be const")
                    .emit();
            }
            _ => {}
        }
    }

    fn no_questions_in_bounds(&self, bounds: &TyParamBounds, where_: &str, is_trait: bool) {
        for bound in bounds {
            if let TraitTyParamBound(ref poly, TraitBoundModifier::Maybe) = *bound {
                let mut err = self.err_handler().struct_span_err(poly.span,
                                    &format!("`?Trait` is not permitted in {}", where_));
                if is_trait {
                    err.note(&format!("traits are `?{}` by default", poly.trait_ref.path));
                }
                err.emit();
            }
        }
    }

    /// matches '-' lit | lit (cf. parser::Parser::parse_pat_literal_maybe_minus),
    /// or path for ranges.
    ///
    /// FIXME: do we want to allow expr -> pattern conversion to create path expressions?
    /// That means making this work:
    ///
    /// ```rust,ignore (FIXME)
    ///     struct S;
    ///     macro_rules! m {
    ///         ($a:expr) => {
    ///             let $a = S;
    ///         }
    ///     }
    ///     m!(S);
    /// ```
    fn check_expr_within_pat(&self, expr: &Expr, allow_paths: bool) {
        match expr.node {
            ExprKind::Lit(..) => {}
            ExprKind::Path(..) if allow_paths => {}
            ExprKind::Unary(UnOp::Neg, ref inner)
                if match inner.node { ExprKind::Lit(_) => true, _ => false } => {}
            _ => self.err_handler().span_err(expr.span, "arbitrary expressions aren't allowed \
                                                         in patterns")
        }
    }
}

impl<'a> Visitor<'a> for AstValidator<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr.node {
            ExprKind::While(.., Some(ident)) |
            ExprKind::Loop(_, Some(ident)) |
            ExprKind::WhileLet(.., Some(ident)) |
            ExprKind::ForLoop(.., Some(ident)) |
            ExprKind::Break(Some(ident), _) |
            ExprKind::Continue(Some(ident)) => {
                self.check_label(ident.node, ident.span);
            }
            _ => {}
        }

        visit::walk_expr(self, expr)
    }

    fn visit_ty(&mut self, ty: &'a Ty) {
        match ty.node {
            TyKind::BareFn(ref bfty) => {
                self.check_decl_no_pat(&bfty.decl, |span, _| {
                    struct_span_err!(self.session, span, E0561,
                                     "patterns aren't allowed in function pointer types").emit();
                });
            }
            TyKind::TraitObject(ref bounds, ..) => {
                let mut any_lifetime_bounds = false;
                for bound in bounds {
                    if let RegionTyParamBound(ref lifetime) = *bound {
                        if any_lifetime_bounds {
                            span_err!(self.session, lifetime.span, E0226,
                                      "only a single explicit lifetime bound is permitted");
                            break;
                        }
                        any_lifetime_bounds = true;
                    }
                }
                self.no_questions_in_bounds(bounds, "trait object types", false);
            }
            TyKind::ImplTrait(ref bounds) => {
                if !bounds.iter()
                          .any(|b| if let TraitTyParamBound(..) = *b { true } else { false }) {
                    self.err_handler().span_err(ty.span, "at least one trait must be specified");
                }
            }
            _ => {}
        }

        visit::walk_ty(self, ty)
    }

    fn visit_use_tree(&mut self, use_tree: &'a UseTree, id: NodeId, _nested: bool) {
        // Check if the path in this `use` is not generic, such as `use foo::bar<T>;` While this
        // can't happen normally thanks to the parser, a generic might sneak in if the `use` is
        // built using a macro.
        //
        // macro_use foo {
        //     ($p:path) => { use $p; }
        // }
        // foo!(bar::baz<T>);
        use_tree.prefix.segments.iter().find(|segment| {
            segment.parameters.is_some()
        }).map(|segment| {
            self.err_handler().span_err(segment.parameters.as_ref().unwrap().span(),
                                        "generic arguments in import path");
        });

        visit::walk_use_tree(self, use_tree, id);
    }

    fn visit_lifetime(&mut self, lifetime: &'a Lifetime) {
        self.check_lifetime(lifetime);
        visit::walk_lifetime(self, lifetime);
    }

    fn visit_item(&mut self, item: &'a Item) {
        match item.node {
            ItemKind::Impl(.., Some(..), ref ty, ref impl_items) => {
                self.invalid_visibility(&item.vis, item.span, None);
                if ty.node == TyKind::Err {
                    self.err_handler()
                        .struct_span_err(item.span, "`impl Trait for .. {}` is an obsolete syntax")
                        .help("use `auto trait Trait {}` instead").emit();
                }
                for impl_item in impl_items {
                    self.invalid_visibility(&impl_item.vis, impl_item.span, None);
                    if let ImplItemKind::Method(ref sig, _) = impl_item.node {
                        self.check_trait_fn_not_const(sig.constness);
                    }
                }
            }
            ItemKind::Impl(.., None, _, _) => {
                self.invalid_visibility(&item.vis,
                                        item.span,
                                        Some("place qualifiers on individual impl items instead"));
            }
            ItemKind::ForeignMod(..) => {
                self.invalid_visibility(&item.vis,
                                        item.span,
                                        Some("place qualifiers on individual foreign items \
                                              instead"));
            }
            ItemKind::Enum(ref def, _) => {
                for variant in &def.variants {
                    self.invalid_non_exhaustive_attribute(variant);
                    for field in variant.node.data.fields() {
                        self.invalid_visibility(&field.vis, field.span, None);
                    }
                }
            }
            ItemKind::Trait(is_auto, _, ref generics, ref bounds, ref trait_items) => {
                if is_auto == IsAuto::Yes {
                    // Auto traits cannot have generics, super traits nor contain items.
                    if generics.is_parameterized() {
                        struct_span_err!(self.session, item.span, E0567,
                                        "auto traits cannot have generic parameters").emit();
                    }
                    if !bounds.is_empty() {
                        struct_span_err!(self.session, item.span, E0568,
                                        "auto traits cannot have super traits").emit();
                    }
                    if !trait_items.is_empty() {
                        struct_span_err!(self.session, item.span, E0380,
                                "auto traits cannot have methods or associated items").emit();
                    }
                }
                self.no_questions_in_bounds(bounds, "supertraits", true);
                for trait_item in trait_items {
                    if let TraitItemKind::Method(ref sig, ref block) = trait_item.node {
                        self.check_trait_fn_not_const(sig.constness);
                        if block.is_none() {
                            self.check_decl_no_pat(&sig.decl, |span, mut_ident| {
                                if mut_ident {
                                    self.session.buffer_lint(
                                        lint::builtin::PATTERNS_IN_FNS_WITHOUT_BODY,
                                        trait_item.id, span,
                                        "patterns aren't allowed in methods without bodies");
                                } else {
                                    struct_span_err!(self.session, span, E0642,
                                        "patterns aren't allowed in methods without bodies").emit();
                                }
                            });
                        }
                    }
                }
            }
            ItemKind::TraitAlias(Generics { ref params, .. }, ..) => {
                for param in params {
                    if let GenericParam::Type(TyParam {
                        ref bounds,
                        ref default,
                        span,
                        ..
                    }) = *param
                    {
                        if !bounds.is_empty() {
                            self.err_handler().span_err(span,
                                                        "type parameters on the left side of a \
                                                         trait alias cannot be bounded");
                        }
                        if !default.is_none() {
                            self.err_handler().span_err(span,
                                                        "type parameters on the left side of a \
                                                         trait alias cannot have defaults");
                        }
                    }
                }
            }
            ItemKind::Mod(_) => {
                // Ensure that `path` attributes on modules are recorded as used (c.f. #35584).
                attr::first_attr_value_str_by_name(&item.attrs, "path");
                if attr::contains_name(&item.attrs, "warn_directory_ownership") {
                    let lint = lint::builtin::LEGACY_DIRECTORY_OWNERSHIP;
                    let msg = "cannot declare a new module at this location";
                    self.session.buffer_lint(lint, item.id, item.span, msg);
                }
            }
            ItemKind::Union(ref vdata, _) => {
                if !vdata.is_struct() {
                    self.err_handler().span_err(item.span,
                                                "tuple and unit unions are not permitted");
                }
                if vdata.fields().len() == 0 {
                    self.err_handler().span_err(item.span,
                                                "unions cannot have zero fields");
                }
            }
            _ => {}
        }

        visit::walk_item(self, item)
    }

    fn visit_foreign_item(&mut self, fi: &'a ForeignItem) {
        match fi.node {
            ForeignItemKind::Fn(ref decl, _) => {
                self.check_decl_no_pat(decl, |span, _| {
                    struct_span_err!(self.session, span, E0130,
                                     "patterns aren't allowed in foreign function declarations")
                        .span_label(span, "pattern not allowed in foreign function").emit();
                });
            }
            ForeignItemKind::Static(..) | ForeignItemKind::Ty => {}
        }

        visit::walk_foreign_item(self, fi)
    }

    fn visit_vis(&mut self, vis: &'a Visibility) {
        match *vis {
            Visibility::Restricted { ref path, .. } => {
                path.segments.iter().find(|segment| segment.parameters.is_some()).map(|segment| {
                    self.err_handler().span_err(segment.parameters.as_ref().unwrap().span(),
                                                "generic arguments in visibility path");
                });
            }
            _ => {}
        }

        visit::walk_vis(self, vis)
    }

    fn visit_generics(&mut self, g: &'a Generics) {
        let mut seen_non_lifetime_param = false;
        let mut seen_default = None;
        for param in &g.params {
            match (param, seen_non_lifetime_param) {
                (&GenericParam::Lifetime(ref ld), true) => {
                    self.err_handler()
                        .span_err(ld.lifetime.span, "lifetime parameters must be leading");
                },
                (&GenericParam::Lifetime(_), false) => {}
                _ => {
                    seen_non_lifetime_param = true;
                }
            }

            if let GenericParam::Type(ref ty_param @ TyParam { default: Some(_), .. }) = *param {
                seen_default = Some(ty_param.span);
            } else if let Some(span) = seen_default {
                self.err_handler()
                    .span_err(span, "type parameters with a default must be trailing");
                break
            }
        }
        for predicate in &g.where_clause.predicates {
            if let WherePredicate::EqPredicate(ref predicate) = *predicate {
                self.err_handler().span_err(predicate.span, "equality constraints are not yet \
                                                             supported in where clauses (#20041)");
            }
        }
        visit::walk_generics(self, g)
    }

    fn visit_pat(&mut self, pat: &'a Pat) {
        match pat.node {
            PatKind::Lit(ref expr) => {
                self.check_expr_within_pat(expr, false);
            }
            PatKind::Range(ref start, ref end, _) => {
                self.check_expr_within_pat(start, true);
                self.check_expr_within_pat(end, true);
            }
            _ => {}
        }

        visit::walk_pat(self, pat)
    }
}

pub fn check_crate(session: &Session, krate: &Crate) {
    visit::walk_crate(&mut AstValidator { session: session }, krate)
}
