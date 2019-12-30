//! Process the potential `cfg` attributes on a module.
//! Also determine if the module should be included in this configuration.
//!
//! This module properly belongs in rustc_expand, but for now it's tied into
//! parsing, so we leave it here to avoid complicated out-of-line dependencies.
//!
//! A principled solution to this wrong location would be to implement [#64197].
//!
//! [#64197]: https://github.com/rust-lang/rust/issues/64197

use crate::{parse_in, validate_attr};
use rustc_errors::Applicability;
use rustc_feature::Features;
use syntax::ast::{self, AttrItem, Attribute, MetaItem};
use syntax::attr;
use syntax::attr::HasAttrs;
use syntax::edition::Edition;
use syntax::feature_gate::{feature_err, get_features};
use syntax::mut_visit::*;
use syntax::ptr::P;
use syntax::sess::ParseSess;
use syntax::util::map_in_place::MapInPlace;
use syntax_pos::symbol::sym;
use syntax_pos::Span;

use smallvec::SmallVec;

/// A folder that strips out items that do not belong in the current configuration.
pub struct StripUnconfigured<'a> {
    pub sess: &'a ParseSess,
    pub features: Option<&'a Features>,
}

// `cfg_attr`-process the crate's attributes and compute the crate's features.
pub fn features(
    mut krate: ast::Crate,
    sess: &ParseSess,
    edition: Edition,
    allow_features: &Option<Vec<String>>,
) -> (ast::Crate, Features) {
    let features;
    {
        let mut strip_unconfigured = StripUnconfigured { sess, features: None };

        let unconfigured_attrs = krate.attrs.clone();
        let err_count = sess.span_diagnostic.err_count();
        if let Some(attrs) = strip_unconfigured.configure(krate.attrs) {
            krate.attrs = attrs;
        } else {
            // the entire crate is unconfigured
            krate.attrs = Vec::new();
            krate.module.items = Vec::new();
            return (krate, Features::default());
        }

        features = get_features(&sess.span_diagnostic, &krate.attrs, edition, allow_features);

        // Avoid reconfiguring malformed `cfg_attr`s
        if err_count == sess.span_diagnostic.err_count() {
            strip_unconfigured.features = Some(&features);
            strip_unconfigured.configure(unconfigured_attrs);
        }
    }

    (krate, features)
}

#[macro_export]
macro_rules! configure {
    ($this:ident, $node:ident) => {
        match $this.configure($node) {
            Some(node) => node,
            None => return Default::default(),
        }
    };
}

const CFG_ATTR_GRAMMAR_HELP: &str = "#[cfg_attr(condition, attribute, other_attribute, ...)]";
const CFG_ATTR_NOTE_REF: &str = "for more information, visit \
    <https://doc.rust-lang.org/reference/conditional-compilation.html\
    #the-cfg_attr-attribute>";

impl<'a> StripUnconfigured<'a> {
    pub fn configure<T: HasAttrs>(&mut self, mut node: T) -> Option<T> {
        self.process_cfg_attrs(&mut node);
        self.in_cfg(node.attrs()).then_some(node)
    }

    /// Parse and expand all `cfg_attr` attributes into a list of attributes
    /// that are within each `cfg_attr` that has a true configuration predicate.
    ///
    /// Gives compiler warnigns if any `cfg_attr` does not contain any
    /// attributes and is in the original source code. Gives compiler errors if
    /// the syntax of any `cfg_attr` is incorrect.
    pub fn process_cfg_attrs<T: HasAttrs>(&mut self, node: &mut T) {
        node.visit_attrs(|attrs| {
            attrs.flat_map_in_place(|attr| self.process_cfg_attr(attr));
        });
    }

    /// Parse and expand a single `cfg_attr` attribute into a list of attributes
    /// when the configuration predicate is true, or otherwise expand into an
    /// empty list of attributes.
    ///
    /// Gives a compiler warning when the `cfg_attr` contains no attributes and
    /// is in the original source file. Gives a compiler error if the syntax of
    /// the attribute is incorrect.
    fn process_cfg_attr(&mut self, attr: Attribute) -> Vec<Attribute> {
        if !attr.has_name(sym::cfg_attr) {
            return vec![attr];
        }

        let (cfg_predicate, expanded_attrs) = match self.parse_cfg_attr(&attr) {
            None => return vec![],
            Some(r) => r,
        };

        // Lint on zero attributes in source.
        if expanded_attrs.is_empty() {
            return vec![attr];
        }

        // At this point we know the attribute is considered used.
        attr::mark_used(&attr);

        if !attr::cfg_matches(&cfg_predicate, self.sess, self.features) {
            return vec![];
        }

        // We call `process_cfg_attr` recursively in case there's a
        // `cfg_attr` inside of another `cfg_attr`. E.g.
        //  `#[cfg_attr(false, cfg_attr(true, some_attr))]`.
        expanded_attrs
            .into_iter()
            .flat_map(|(item, span)| {
                let attr = attr::mk_attr_from_item(attr.style, item, span);
                self.process_cfg_attr(attr)
            })
            .collect()
    }

    fn parse_cfg_attr(&self, attr: &Attribute) -> Option<(MetaItem, Vec<(AttrItem, Span)>)> {
        match attr.get_normal_item().args {
            ast::MacArgs::Delimited(dspan, delim, ref tts) if !tts.is_empty() => {
                let msg = "wrong `cfg_attr` delimiters";
                validate_attr::check_meta_bad_delim(self.sess, dspan, delim, msg);
                match parse_in(self.sess, tts.clone(), "`cfg_attr` input", |p| p.parse_cfg_attr()) {
                    Ok(r) => return Some(r),
                    Err(mut e) => e
                        .help(&format!("the valid syntax is `{}`", CFG_ATTR_GRAMMAR_HELP))
                        .note(CFG_ATTR_NOTE_REF)
                        .emit(),
                }
            }
            _ => self.error_malformed_cfg_attr_missing(attr.span),
        }
        None
    }

    fn error_malformed_cfg_attr_missing(&self, span: Span) {
        self.sess
            .span_diagnostic
            .struct_span_err(span, "malformed `cfg_attr` attribute input")
            .span_suggestion(
                span,
                "missing condition and attribute",
                CFG_ATTR_GRAMMAR_HELP.to_string(),
                Applicability::HasPlaceholders,
            )
            .note(CFG_ATTR_NOTE_REF)
            .emit();
    }

    /// Determines if a node with the given attributes should be included in this configuration.
    pub fn in_cfg(&self, attrs: &[Attribute]) -> bool {
        attrs.iter().all(|attr| {
            if !is_cfg(attr) {
                return true;
            }

            let error = |span, msg, suggestion: &str| {
                let mut err = self.sess.span_diagnostic.struct_span_err(span, msg);
                if !suggestion.is_empty() {
                    err.span_suggestion(
                        span,
                        "expected syntax is",
                        suggestion.into(),
                        Applicability::MaybeIncorrect,
                    );
                }
                err.emit();
                true
            };

            let meta_item = match validate_attr::parse_meta(self.sess, attr) {
                Ok(meta_item) => meta_item,
                Err(mut err) => {
                    err.emit();
                    return true;
                }
            };
            let nested_meta_items = if let Some(nested_meta_items) = meta_item.meta_item_list() {
                nested_meta_items
            } else {
                return error(
                    meta_item.span,
                    "`cfg` is not followed by parentheses",
                    "cfg(/* predicate */)",
                );
            };

            if nested_meta_items.is_empty() {
                return error(meta_item.span, "`cfg` predicate is not specified", "");
            } else if nested_meta_items.len() > 1 {
                return error(
                    nested_meta_items.last().unwrap().span(),
                    "multiple `cfg` predicates are specified",
                    "",
                );
            }

            match nested_meta_items[0].meta_item() {
                Some(meta_item) => attr::cfg_matches(meta_item, self.sess, self.features),
                None => error(
                    nested_meta_items[0].span(),
                    "`cfg` predicate key cannot be a literal",
                    "",
                ),
            }
        })
    }

    /// Visit attributes on expression and statements (but not attributes on items in blocks).
    fn visit_expr_attrs(&mut self, attrs: &[Attribute]) {
        // flag the offending attributes
        for attr in attrs.iter() {
            self.maybe_emit_expr_attr_err(attr);
        }
    }

    /// If attributes are not allowed on expressions, emit an error for `attr`
    pub fn maybe_emit_expr_attr_err(&self, attr: &Attribute) {
        if !self.features.map(|features| features.stmt_expr_attributes).unwrap_or(true) {
            let mut err = feature_err(
                self.sess,
                sym::stmt_expr_attributes,
                attr.span,
                "attributes on expressions are experimental",
            );

            if attr.is_doc_comment() {
                err.help("`///` is for documentation comments. For a plain comment, use `//`.");
            }

            err.emit();
        }
    }

    pub fn configure_foreign_mod(&mut self, foreign_mod: &mut ast::ForeignMod) {
        let ast::ForeignMod { abi: _, items } = foreign_mod;
        items.flat_map_in_place(|item| self.configure(item));
    }

    pub fn configure_generic_params(&mut self, params: &mut Vec<ast::GenericParam>) {
        params.flat_map_in_place(|param| self.configure(param));
    }

    fn configure_variant_data(&mut self, vdata: &mut ast::VariantData) {
        match vdata {
            ast::VariantData::Struct(fields, ..) | ast::VariantData::Tuple(fields, _) => {
                fields.flat_map_in_place(|field| self.configure(field))
            }
            ast::VariantData::Unit(_) => {}
        }
    }

    pub fn configure_item_kind(&mut self, item: &mut ast::ItemKind) {
        match item {
            ast::ItemKind::Struct(def, _generics) | ast::ItemKind::Union(def, _generics) => {
                self.configure_variant_data(def)
            }
            ast::ItemKind::Enum(ast::EnumDef { variants }, _generics) => {
                variants.flat_map_in_place(|variant| self.configure(variant));
                for variant in variants {
                    self.configure_variant_data(&mut variant.data);
                }
            }
            _ => {}
        }
    }

    pub fn configure_expr_kind(&mut self, expr_kind: &mut ast::ExprKind) {
        match expr_kind {
            ast::ExprKind::Match(_m, arms) => {
                arms.flat_map_in_place(|arm| self.configure(arm));
            }
            ast::ExprKind::Struct(_path, fields, _base) => {
                fields.flat_map_in_place(|field| self.configure(field));
            }
            _ => {}
        }
    }

    pub fn configure_expr(&mut self, expr: &mut P<ast::Expr>) {
        self.visit_expr_attrs(expr.attrs());

        // If an expr is valid to cfg away it will have been removed by the
        // outer stmt or expression folder before descending in here.
        // Anything else is always required, and thus has to error out
        // in case of a cfg attr.
        //
        // N.B., this is intentionally not part of the visit_expr() function
        //     in order for filter_map_expr() to be able to avoid this check
        if let Some(attr) = expr.attrs().iter().find(|a| is_cfg(a)) {
            let msg = "removing an expression is not supported in this position";
            self.sess.span_diagnostic.span_err(attr.span, msg);
        }

        self.process_cfg_attrs(expr)
    }

    pub fn configure_pat(&mut self, pat: &mut P<ast::Pat>) {
        if let ast::PatKind::Struct(_path, fields, _etc) = &mut pat.kind {
            fields.flat_map_in_place(|field| self.configure(field));
        }
    }

    pub fn configure_fn_decl(&mut self, fn_decl: &mut ast::FnDecl) {
        fn_decl.inputs.flat_map_in_place(|arg| self.configure(arg));
    }
}

impl<'a> MutVisitor for StripUnconfigured<'a> {
    fn visit_foreign_mod(&mut self, foreign_mod: &mut ast::ForeignMod) {
        self.configure_foreign_mod(foreign_mod);
        noop_visit_foreign_mod(foreign_mod, self);
    }

    fn visit_item_kind(&mut self, item: &mut ast::ItemKind) {
        self.configure_item_kind(item);
        noop_visit_item_kind(item, self);
    }

    fn visit_expr(&mut self, expr: &mut P<ast::Expr>) {
        self.configure_expr(expr);
        self.configure_expr_kind(&mut expr.kind);
        noop_visit_expr(expr, self);
    }

    fn filter_map_expr(&mut self, expr: P<ast::Expr>) -> Option<P<ast::Expr>> {
        let mut expr = configure!(self, expr);
        self.configure_expr_kind(&mut expr.kind);
        noop_visit_expr(&mut expr, self);
        Some(expr)
    }

    fn flat_map_stmt(&mut self, stmt: ast::Stmt) -> SmallVec<[ast::Stmt; 1]> {
        noop_flat_map_stmt(configure!(self, stmt), self)
    }

    fn flat_map_item(&mut self, item: P<ast::Item>) -> SmallVec<[P<ast::Item>; 1]> {
        noop_flat_map_item(configure!(self, item), self)
    }

    fn flat_map_impl_item(&mut self, item: ast::AssocItem) -> SmallVec<[ast::AssocItem; 1]> {
        noop_flat_map_assoc_item(configure!(self, item), self)
    }

    fn flat_map_trait_item(&mut self, item: ast::AssocItem) -> SmallVec<[ast::AssocItem; 1]> {
        noop_flat_map_assoc_item(configure!(self, item), self)
    }

    fn visit_mac(&mut self, _mac: &mut ast::Mac) {
        // Don't configure interpolated AST (cf. issue #34171).
        // Interpolated AST will get configured once the surrounding tokens are parsed.
    }

    fn visit_pat(&mut self, pat: &mut P<ast::Pat>) {
        self.configure_pat(pat);
        noop_visit_pat(pat, self)
    }

    fn visit_fn_decl(&mut self, mut fn_decl: &mut P<ast::FnDecl>) {
        self.configure_fn_decl(&mut fn_decl);
        noop_visit_fn_decl(fn_decl, self);
    }
}

fn is_cfg(attr: &Attribute) -> bool {
    attr.check_name(sym::cfg)
}

/// Process the potential `cfg` attributes on a module.
/// Also determine if the module should be included in this configuration.
pub fn process_configure_mod(
    sess: &ParseSess,
    cfg_mods: bool,
    attrs: &[Attribute],
) -> (bool, Vec<Attribute>) {
    // Don't perform gated feature checking.
    let mut strip_unconfigured = StripUnconfigured { sess, features: None };
    let mut attrs = attrs.to_owned();
    strip_unconfigured.process_cfg_attrs(&mut attrs);
    (!cfg_mods || strip_unconfigured.in_cfg(&attrs), attrs)
}
