use super::diagnostics::{dummy_arg, ConsumeClosingDelim, Error};
use super::ty::{AllowPlus, RecoverQPath};
use super::{FollowedByType, Parser, PathStyle};

use crate::maybe_whole;

use rustc_ast_pretty::pprust;
use rustc_errors::{struct_span_err, Applicability, DiagnosticBuilder, PResult, StashKey};
use rustc_span::source_map::{self, Span};
use rustc_span::symbol::{kw, sym, Symbol};
use rustc_span::BytePos;
use syntax::ast::{self, AttrKind, AttrStyle, AttrVec, Attribute, Ident, DUMMY_NODE_ID};
use syntax::ast::{AssocItem, AssocItemKind, Item, ItemKind, UseTree, UseTreeKind};
use syntax::ast::{Async, Const, Defaultness, IsAuto, PathSegment, StrLit, Unsafe};
use syntax::ast::{BindingMode, Block, FnDecl, FnSig, Mac, MacArgs, MacDelimiter, Param, SelfKind};
use syntax::ast::{EnumDef, Generics, StructField, TraitRef, Ty, TyKind, Variant, VariantData};
use syntax::ast::{FnHeader, ForeignItem, ForeignItemKind, Mutability, Visibility, VisibilityKind};
use syntax::ptr::P;
use syntax::token;
use syntax::tokenstream::{DelimSpan, TokenStream, TokenTree};

use log::debug;
use std::mem;

pub(super) type ItemInfo = (Ident, ItemKind, Option<Vec<Attribute>>);

impl<'a> Parser<'a> {
    pub fn parse_item(&mut self) -> PResult<'a, Option<P<Item>>> {
        let attrs = self.parse_outer_attributes()?;
        self.parse_item_(attrs, true, false)
    }

    pub(super) fn parse_item_(
        &mut self,
        attrs: Vec<Attribute>,
        macros_allowed: bool,
        attributes_allowed: bool,
    ) -> PResult<'a, Option<P<Item>>> {
        let mut unclosed_delims = vec![];
        let (ret, tokens) = self.collect_tokens(|this| {
            let item = this.parse_item_implementation(attrs, macros_allowed, attributes_allowed);
            unclosed_delims.append(&mut this.unclosed_delims);
            item
        })?;
        self.unclosed_delims.append(&mut unclosed_delims);

        // Once we've parsed an item and recorded the tokens we got while
        // parsing we may want to store `tokens` into the item we're about to
        // return. Note, though, that we specifically didn't capture tokens
        // related to outer attributes. The `tokens` field here may later be
        // used with procedural macros to convert this item back into a token
        // stream, but during expansion we may be removing attributes as we go
        // along.
        //
        // If we've got inner attributes then the `tokens` we've got above holds
        // these inner attributes. If an inner attribute is expanded we won't
        // actually remove it from the token stream, so we'll just keep yielding
        // it (bad!). To work around this case for now we just avoid recording
        // `tokens` if we detect any inner attributes. This should help keep
        // expansion correct, but we should fix this bug one day!
        Ok(ret.map(|item| {
            item.map(|mut i| {
                if !i.attrs.iter().any(|attr| attr.style == AttrStyle::Inner) {
                    i.tokens = Some(tokens);
                }
                i
            })
        }))
    }

    /// Parses one of the items allowed by the flags.
    fn parse_item_implementation(
        &mut self,
        mut attrs: Vec<Attribute>,
        macros_allowed: bool,
        attributes_allowed: bool,
    ) -> PResult<'a, Option<P<Item>>> {
        maybe_whole!(self, NtItem, |item| {
            let mut item = item;
            mem::swap(&mut item.attrs, &mut attrs);
            item.attrs.extend(attrs);
            Some(item)
        });

        let lo = self.token.span;

        let vis = self.parse_visibility(FollowedByType::No)?;

        if self.eat_keyword(kw::Use) {
            // USE ITEM
            let item_ = ItemKind::Use(P(self.parse_use_tree()?));
            self.expect_semi()?;

            let span = lo.to(self.prev_span);
            let item = self.mk_item(span, Ident::invalid(), item_, vis, attrs);
            return Ok(Some(item));
        }

        if self.check_fn_front_matter() {
            // FUNCTION ITEM
            let (ident, sig, generics, body) = self.parse_fn(&mut false, &mut attrs, |_| true)?;
            let kind = ItemKind::Fn(sig, generics, body);
            return self.mk_item_with_info(attrs, lo, vis, (ident, kind, None));
        }

        if self.eat_keyword(kw::Extern) {
            if self.eat_keyword(kw::Crate) {
                // EXTERN CRATE
                return Ok(Some(self.parse_item_extern_crate(lo, vis, attrs)?));
            }
            // EXTERN BLOCK
            let abi = self.parse_abi();
            return Ok(Some(self.parse_item_foreign_mod(lo, abi, vis, attrs)?));
        }

        if self.is_static_global() {
            // STATIC ITEM
            self.bump();
            let m = self.parse_mutability();
            let info = self.parse_item_const(Some(m))?;
            return self.mk_item_with_info(attrs, lo, vis, info);
        }

        if let Const::Yes(const_span) = self.parse_constness() {
            // CONST ITEM
            if self.eat_keyword(kw::Mut) {
                let prev_span = self.prev_span;
                self.struct_span_err(prev_span, "const globals cannot be mutable")
                    .span_label(prev_span, "cannot be mutable")
                    .span_suggestion(
                        const_span,
                        "you might want to declare a static instead",
                        "static".to_owned(),
                        Applicability::MaybeIncorrect,
                    )
                    .emit();
            }

            let info = self.parse_item_const(None)?;
            return self.mk_item_with_info(attrs, lo, vis, info);
        }

        if self.check_keyword(kw::Unsafe) && self.is_keyword_ahead(1, &[kw::Trait, kw::Auto]) {
            // UNSAFE TRAIT ITEM
            let unsafety = self.parse_unsafety();
            let info = self.parse_item_trait(lo, unsafety)?;
            return self.mk_item_with_info(attrs, lo, vis, info);
        }

        if self.check_keyword(kw::Impl)
            || self.check_keyword(kw::Unsafe) && self.is_keyword_ahead(1, &[kw::Impl])
            || self.check_keyword(kw::Default) && self.is_keyword_ahead(1, &[kw::Impl, kw::Unsafe])
        {
            // IMPL ITEM
            let defaultness = self.parse_defaultness();
            let unsafety = self.parse_unsafety();
            self.expect_keyword(kw::Impl)?;
            let info = self.parse_item_impl(unsafety, defaultness)?;
            return self.mk_item_with_info(attrs, lo, vis, info);
        }

        if self.eat_keyword(kw::Mod) {
            // MODULE ITEM
            let info = self.parse_item_mod(&attrs[..])?;
            return self.mk_item_with_info(attrs, lo, vis, info);
        }

        if self.eat_keyword(kw::Type) {
            // TYPE ITEM
            let (ident, ty, generics) = self.parse_type_alias()?;
            let kind = ItemKind::TyAlias(ty, generics);
            return self.mk_item_with_info(attrs, lo, vis, (ident, kind, None));
        }

        if self.eat_keyword(kw::Enum) {
            // ENUM ITEM
            let info = self.parse_item_enum()?;
            return self.mk_item_with_info(attrs, lo, vis, info);
        }

        if self.check_keyword(kw::Trait)
            || (self.check_keyword(kw::Auto) && self.is_keyword_ahead(1, &[kw::Trait]))
        {
            // TRAIT ITEM
            let info = self.parse_item_trait(lo, Unsafe::No)?;
            return self.mk_item_with_info(attrs, lo, vis, info);
        }

        if self.eat_keyword(kw::Struct) {
            // STRUCT ITEM
            let info = self.parse_item_struct()?;
            return self.mk_item_with_info(attrs, lo, vis, info);
        }

        if self.is_union_item() {
            // UNION ITEM
            self.bump();
            let info = self.parse_item_union()?;
            return self.mk_item_with_info(attrs, lo, vis, info);
        }

        if let Some(macro_def) = self.eat_macro_def(&attrs, &vis, lo)? {
            return Ok(Some(macro_def));
        }

        // Verify whether we have encountered a struct or method definition where the user forgot to
        // add the `struct` or `fn` keyword after writing `pub`: `pub S {}`
        if vis.node.is_pub() && self.check_ident() && self.look_ahead(1, |t| *t != token::Not) {
            // Space between `pub` keyword and the identifier
            //
            //     pub   S {}
            //        ^^^ `sp` points here
            let sp = self.prev_span.between(self.token.span);
            let full_sp = self.prev_span.to(self.token.span);
            let ident_sp = self.token.span;
            if self.look_ahead(1, |t| *t == token::OpenDelim(token::Brace)) {
                // possible public struct definition where `struct` was forgotten
                let ident = self.parse_ident().unwrap();
                let msg = format!("add `struct` here to parse `{}` as a public struct", ident);
                let mut err = self.struct_span_err(sp, "missing `struct` for struct definition");
                err.span_suggestion_short(
                    sp,
                    &msg,
                    " struct ".into(),
                    Applicability::MaybeIncorrect, // speculative
                );
                return Err(err);
            } else if self.look_ahead(1, |t| *t == token::OpenDelim(token::Paren)) {
                let ident = self.parse_ident().unwrap();
                self.bump(); // `(`
                let kw_name = self.recover_first_param();
                self.consume_block(token::Paren, ConsumeClosingDelim::Yes);
                let (kw, kw_name, ambiguous) = if self.check(&token::RArrow) {
                    self.eat_to_tokens(&[&token::OpenDelim(token::Brace)]);
                    self.bump(); // `{`
                    ("fn", kw_name, false)
                } else if self.check(&token::OpenDelim(token::Brace)) {
                    self.bump(); // `{`
                    ("fn", kw_name, false)
                } else if self.check(&token::Colon) {
                    let kw = "struct";
                    (kw, kw, false)
                } else {
                    ("fn` or `struct", "function or struct", true)
                };

                let msg = format!("missing `{}` for {} definition", kw, kw_name);
                let mut err = self.struct_span_err(sp, &msg);
                if !ambiguous {
                    self.consume_block(token::Brace, ConsumeClosingDelim::Yes);
                    let suggestion =
                        format!("add `{}` here to parse `{}` as a public {}", kw, ident, kw_name);
                    err.span_suggestion_short(
                        sp,
                        &suggestion,
                        format!(" {} ", kw),
                        Applicability::MachineApplicable,
                    );
                } else {
                    if let Ok(snippet) = self.span_to_snippet(ident_sp) {
                        err.span_suggestion(
                            full_sp,
                            "if you meant to call a macro, try",
                            format!("{}!", snippet),
                            // this is the `ambiguous` conditional branch
                            Applicability::MaybeIncorrect,
                        );
                    } else {
                        err.help(
                            "if you meant to call a macro, remove the `pub` \
                                  and add a trailing `!` after the identifier",
                        );
                    }
                }
                return Err(err);
            } else if self.look_ahead(1, |t| *t == token::Lt) {
                let ident = self.parse_ident().unwrap();
                self.eat_to_tokens(&[&token::Gt]);
                self.bump(); // `>`
                let (kw, kw_name, ambiguous) = if self.eat(&token::OpenDelim(token::Paren)) {
                    ("fn", self.recover_first_param(), false)
                } else if self.check(&token::OpenDelim(token::Brace)) {
                    ("struct", "struct", false)
                } else {
                    ("fn` or `struct", "function or struct", true)
                };
                let msg = format!("missing `{}` for {} definition", kw, kw_name);
                let mut err = self.struct_span_err(sp, &msg);
                if !ambiguous {
                    err.span_suggestion_short(
                        sp,
                        &format!("add `{}` here to parse `{}` as a public {}", kw, ident, kw_name),
                        format!(" {} ", kw),
                        Applicability::MachineApplicable,
                    );
                }
                return Err(err);
            }
        }
        self.parse_macro_use_or_failure(attrs, macros_allowed, attributes_allowed, lo, vis)
    }

    pub(super) fn mk_item_with_info(
        &self,
        attrs: Vec<Attribute>,
        lo: Span,
        vis: Visibility,
        info: ItemInfo,
    ) -> PResult<'a, Option<P<Item>>> {
        let (ident, item, extra_attrs) = info;
        let span = lo.to(self.prev_span);
        let attrs = Self::maybe_append(attrs, extra_attrs);
        Ok(Some(self.mk_item(span, ident, item, vis, attrs)))
    }

    fn maybe_append<T>(mut lhs: Vec<T>, mut rhs: Option<Vec<T>>) -> Vec<T> {
        if let Some(ref mut rhs) = rhs {
            lhs.append(rhs);
        }
        lhs
    }

    /// This is the fall-through for parsing items.
    fn parse_macro_use_or_failure(
        &mut self,
        attrs: Vec<Attribute>,
        macros_allowed: bool,
        attributes_allowed: bool,
        lo: Span,
        visibility: Visibility,
    ) -> PResult<'a, Option<P<Item>>> {
        if macros_allowed
            && self.token.is_path_start()
            && !(self.is_async_fn() && self.token.span.rust_2015())
        {
            // MACRO INVOCATION ITEM

            let prev_span = self.prev_span;
            self.complain_if_pub_macro(&visibility.node, prev_span);

            // Item macro
            let path = self.parse_path(PathStyle::Mod)?;
            self.expect(&token::Not)?;
            let args = self.parse_mac_args()?;
            if args.need_semicolon() && !self.eat(&token::Semi) {
                self.report_invalid_macro_expansion_item();
            }

            let hi = self.prev_span;
            let mac = Mac { path, args, prior_type_ascription: self.last_type_ascription };
            let item =
                self.mk_item(lo.to(hi), Ident::invalid(), ItemKind::Mac(mac), visibility, attrs);
            return Ok(Some(item));
        }

        // FAILURE TO PARSE ITEM
        match visibility.node {
            VisibilityKind::Inherited => {}
            _ => return Err(self.struct_span_err(self.prev_span, "unmatched visibility `pub`")),
        }

        if !attributes_allowed && !attrs.is_empty() {
            self.expected_item_err(&attrs)?;
        }
        Ok(None)
    }

    /// Emits an expected-item-after-attributes error.
    fn expected_item_err(&mut self, attrs: &[Attribute]) -> PResult<'a, ()> {
        let message = match attrs.last() {
            Some(&Attribute { kind: AttrKind::DocComment(_), .. }) => {
                "expected item after doc comment"
            }
            _ => "expected item after attributes",
        };

        let mut err = self.struct_span_err(self.prev_span, message);
        if attrs.last().unwrap().is_doc_comment() {
            err.span_label(self.prev_span, "this doc comment doesn't document anything");
        }
        Err(err)
    }

    pub(super) fn is_async_fn(&self) -> bool {
        self.token.is_keyword(kw::Async) && self.is_keyword_ahead(1, &[kw::Fn])
    }

    /// Parses a macro invocation inside a `trait`, `impl` or `extern` block.
    fn parse_assoc_macro_invoc(
        &mut self,
        item_kind: &str,
        vis: Option<&Visibility>,
        at_end: &mut bool,
    ) -> PResult<'a, Option<Mac>> {
        if self.token.is_path_start() && !(self.is_async_fn() && self.token.span.rust_2015()) {
            let prev_span = self.prev_span;
            let path = self.parse_path(PathStyle::Mod)?;

            if path.segments.len() == 1 {
                if !self.eat(&token::Not) {
                    return Err(self.missing_assoc_item_kind_err(item_kind, prev_span));
                }
            } else {
                self.expect(&token::Not)?;
            }

            if let Some(vis) = vis {
                self.complain_if_pub_macro(&vis.node, prev_span);
            }

            *at_end = true;

            // eat a matched-delimiter token tree:
            let args = self.parse_mac_args()?;
            if args.need_semicolon() {
                self.expect_semi()?;
            }

            Ok(Some(Mac { path, args, prior_type_ascription: self.last_type_ascription }))
        } else {
            Ok(None)
        }
    }

    fn missing_assoc_item_kind_err(
        &self,
        item_type: &str,
        prev_span: Span,
    ) -> DiagnosticBuilder<'a> {
        let expected_kinds = if item_type == "extern" {
            "missing `fn`, `type`, or `static`"
        } else {
            "missing `fn`, `type`, or `const`"
        };

        // Given this code `path(`, it seems like this is not
        // setting the visibility of a macro invocation, but rather
        // a mistyped method declaration.
        // Create a diagnostic pointing out that `fn` is missing.
        //
        // x |     pub path(&self) {
        //   |        ^ missing `fn`, `type`, or `const`
        //     pub  path(
        //        ^^ `sp` below will point to this
        let sp = prev_span.between(self.prev_span);
        let mut err = self
            .struct_span_err(sp, &format!("{} for {}-item declaration", expected_kinds, item_type));
        err.span_label(sp, expected_kinds);
        err
    }

    /// Parses an implementation item, `impl` keyword is already parsed.
    ///
    ///    impl<'a, T> TYPE { /* impl items */ }
    ///    impl<'a, T> TRAIT for TYPE { /* impl items */ }
    ///    impl<'a, T> !TRAIT for TYPE { /* impl items */ }
    ///    impl<'a, T> const TRAIT for TYPE { /* impl items */ }
    ///
    /// We actually parse slightly more relaxed grammar for better error reporting and recovery.
    ///   `impl` GENERICS `const`? `!`? TYPE `for`? (TYPE | `..`) (`where` PREDICATES)? `{` BODY `}`
    ///   `impl` GENERICS `const`? `!`? TYPE (`where` PREDICATES)? `{` BODY `}`
    fn parse_item_impl(
        &mut self,
        unsafety: Unsafe,
        defaultness: Defaultness,
    ) -> PResult<'a, ItemInfo> {
        // First, parse generic parameters if necessary.
        let mut generics = if self.choose_generics_over_qpath() {
            self.parse_generics()?
        } else {
            let mut generics = Generics::default();
            // impl A for B {}
            //    /\ this is where `generics.span` should point when there are no type params.
            generics.span = self.prev_span.shrink_to_hi();
            generics
        };

        let constness = self.parse_constness();
        if let Const::Yes(span) = constness {
            self.sess.gated_spans.gate(sym::const_trait_impl, span);
        }

        // Disambiguate `impl !Trait for Type { ... }` and `impl ! { ... }` for the never type.
        let polarity = if self.check(&token::Not) && self.look_ahead(1, |t| t.can_begin_type()) {
            self.bump(); // `!`
            ast::ImplPolarity::Negative
        } else {
            ast::ImplPolarity::Positive
        };

        // Parse both types and traits as a type, then reinterpret if necessary.
        let err_path = |span| ast::Path::from_ident(Ident::new(kw::Invalid, span));
        let ty_first = if self.token.is_keyword(kw::For) && self.look_ahead(1, |t| t != &token::Lt)
        {
            let span = self.prev_span.between(self.token.span);
            self.struct_span_err(span, "missing trait in a trait impl").emit();
            P(Ty { kind: TyKind::Path(None, err_path(span)), span, id: DUMMY_NODE_ID })
        } else {
            self.parse_ty()?
        };

        // If `for` is missing we try to recover.
        let has_for = self.eat_keyword(kw::For);
        let missing_for_span = self.prev_span.between(self.token.span);

        let ty_second = if self.token == token::DotDot {
            // We need to report this error after `cfg` expansion for compatibility reasons
            self.bump(); // `..`, do not add it to expected tokens
            Some(self.mk_ty(self.prev_span, TyKind::Err))
        } else if has_for || self.token.can_begin_type() {
            Some(self.parse_ty()?)
        } else {
            None
        };

        generics.where_clause = self.parse_where_clause()?;

        let (impl_items, attrs) = self.parse_impl_body()?;

        let item_kind = match ty_second {
            Some(ty_second) => {
                // impl Trait for Type
                if !has_for {
                    self.struct_span_err(missing_for_span, "missing `for` in a trait impl")
                        .span_suggestion_short(
                            missing_for_span,
                            "add `for` here",
                            " for ".to_string(),
                            Applicability::MachineApplicable,
                        )
                        .emit();
                }

                let ty_first = ty_first.into_inner();
                let path = match ty_first.kind {
                    // This notably includes paths passed through `ty` macro fragments (#46438).
                    TyKind::Path(None, path) => path,
                    _ => {
                        self.struct_span_err(ty_first.span, "expected a trait, found type").emit();
                        err_path(ty_first.span)
                    }
                };
                let trait_ref = TraitRef { path, ref_id: ty_first.id };

                ItemKind::Impl {
                    unsafety,
                    polarity,
                    defaultness,
                    constness,
                    generics,
                    of_trait: Some(trait_ref),
                    self_ty: ty_second,
                    items: impl_items,
                }
            }
            None => {
                // impl Type
                ItemKind::Impl {
                    unsafety,
                    polarity,
                    defaultness,
                    constness,
                    generics,
                    of_trait: None,
                    self_ty: ty_first,
                    items: impl_items,
                }
            }
        };

        Ok((Ident::invalid(), item_kind, Some(attrs)))
    }

    fn parse_impl_body(&mut self) -> PResult<'a, (Vec<P<AssocItem>>, Vec<Attribute>)> {
        self.expect(&token::OpenDelim(token::Brace))?;
        let attrs = self.parse_inner_attributes()?;

        let mut impl_items = Vec::new();
        while !self.eat(&token::CloseDelim(token::Brace)) {
            let mut at_end = false;
            match self.parse_impl_item(&mut at_end) {
                Ok(impl_item) => impl_items.push(impl_item),
                Err(mut err) => {
                    err.emit();
                    if !at_end {
                        self.consume_block(token::Brace, ConsumeClosingDelim::Yes);
                        break;
                    }
                }
            }
        }
        Ok((impl_items, attrs))
    }

    /// Parses defaultness (i.e., `default` or nothing).
    fn parse_defaultness(&mut self) -> Defaultness {
        // `pub` is included for better error messages
        if self.check_keyword(kw::Default)
            && self.is_keyword_ahead(
                1,
                &[
                    kw::Impl,
                    kw::Const,
                    kw::Async,
                    kw::Fn,
                    kw::Unsafe,
                    kw::Extern,
                    kw::Type,
                    kw::Pub,
                ],
            )
        {
            self.bump(); // `default`
            Defaultness::Default
        } else {
            Defaultness::Final
        }
    }

    /// Parses `auto? trait Foo { ... }` or `trait Foo = Bar;`.
    fn parse_item_trait(&mut self, lo: Span, unsafety: Unsafe) -> PResult<'a, ItemInfo> {
        // Parse optional `auto` prefix.
        let is_auto = if self.eat_keyword(kw::Auto) { IsAuto::Yes } else { IsAuto::No };

        self.expect_keyword(kw::Trait)?;
        let ident = self.parse_ident()?;
        let mut tps = self.parse_generics()?;

        // Parse optional colon and supertrait bounds.
        let had_colon = self.eat(&token::Colon);
        let span_at_colon = self.prev_span;
        let bounds =
            if had_colon { self.parse_generic_bounds(Some(self.prev_span))? } else { Vec::new() };

        let span_before_eq = self.prev_span;
        if self.eat(&token::Eq) {
            // It's a trait alias.
            if had_colon {
                let span = span_at_colon.to(span_before_eq);
                self.struct_span_err(span, "bounds are not allowed on trait aliases").emit();
            }

            let bounds = self.parse_generic_bounds(None)?;
            tps.where_clause = self.parse_where_clause()?;
            self.expect_semi()?;

            let whole_span = lo.to(self.prev_span);
            if is_auto == IsAuto::Yes {
                let msg = "trait aliases cannot be `auto`";
                self.struct_span_err(whole_span, msg).span_label(whole_span, msg).emit();
            }
            if let Unsafe::Yes(_) = unsafety {
                let msg = "trait aliases cannot be `unsafe`";
                self.struct_span_err(whole_span, msg).span_label(whole_span, msg).emit();
            }

            self.sess.gated_spans.gate(sym::trait_alias, whole_span);

            Ok((ident, ItemKind::TraitAlias(tps, bounds), None))
        } else {
            // It's a normal trait.
            tps.where_clause = self.parse_where_clause()?;
            self.expect(&token::OpenDelim(token::Brace))?;
            let mut trait_items = vec![];
            while !self.eat(&token::CloseDelim(token::Brace)) {
                if let token::DocComment(_) = self.token.kind {
                    if self.look_ahead(1, |tok| tok == &token::CloseDelim(token::Brace)) {
                        struct_span_err!(
                            self.diagnostic(),
                            self.token.span,
                            E0584,
                            "found a documentation comment that doesn't document anything",
                        )
                        .help(
                            "doc comments must come before what they document, maybe a \
                            comment was intended with `//`?",
                        )
                        .emit();
                        self.bump();
                        continue;
                    }
                }
                let mut at_end = false;
                match self.parse_trait_item(&mut at_end) {
                    Ok(item) => trait_items.push(item),
                    Err(mut e) => {
                        e.emit();
                        if !at_end {
                            self.consume_block(token::Brace, ConsumeClosingDelim::Yes);
                            break;
                        }
                    }
                }
            }
            Ok((ident, ItemKind::Trait(is_auto, unsafety, tps, bounds, trait_items), None))
        }
    }

    pub fn parse_impl_item(&mut self, at_end: &mut bool) -> PResult<'a, P<AssocItem>> {
        maybe_whole!(self, NtImplItem, |x| x);
        self.parse_assoc_item(at_end, |_| true)
    }

    pub fn parse_trait_item(&mut self, at_end: &mut bool) -> PResult<'a, P<AssocItem>> {
        maybe_whole!(self, NtTraitItem, |x| x);
        // This is somewhat dubious; We don't want to allow
        // param names to be left off if there is a definition...
        //
        // We don't allow param names to be left off in edition 2018.
        self.parse_assoc_item(at_end, |t| t.span.rust_2018())
    }

    /// Parses associated items.
    fn parse_assoc_item(
        &mut self,
        at_end: &mut bool,
        req_name: fn(&token::Token) -> bool,
    ) -> PResult<'a, P<AssocItem>> {
        let attrs = self.parse_outer_attributes()?;
        let mut unclosed_delims = vec![];
        let (mut item, tokens) = self.collect_tokens(|this| {
            let item = this.parse_assoc_item_(at_end, attrs, req_name);
            unclosed_delims.append(&mut this.unclosed_delims);
            item
        })?;
        self.unclosed_delims.append(&mut unclosed_delims);
        // See `parse_item` for why this clause is here.
        if !item.attrs.iter().any(|attr| attr.style == AttrStyle::Inner) {
            item.tokens = Some(tokens);
        }
        Ok(P(item))
    }

    fn parse_assoc_item_(
        &mut self,
        at_end: &mut bool,
        mut attrs: Vec<Attribute>,
        req_name: fn(&token::Token) -> bool,
    ) -> PResult<'a, AssocItem> {
        let lo = self.token.span;
        let vis = self.parse_visibility(FollowedByType::No)?;
        let defaultness = self.parse_defaultness();
        let (name, kind, generics) = if self.eat_keyword(kw::Type) {
            self.parse_assoc_ty()?
        } else if self.check_fn_front_matter() {
            let (ident, sig, generics, body) = self.parse_fn(at_end, &mut attrs, req_name)?;
            (ident, AssocItemKind::Fn(sig, body), generics)
        } else if let Some(mac) = self.parse_assoc_macro_invoc("associated", Some(&vis), at_end)? {
            (Ident::invalid(), AssocItemKind::Macro(mac), Generics::default())
        } else {
            self.parse_assoc_const()?
        };

        Ok(AssocItem {
            id: DUMMY_NODE_ID,
            span: lo.to(self.prev_span),
            ident: name,
            attrs,
            vis,
            defaultness,
            generics,
            kind,
            tokens: None,
        })
    }

    /// This parses the grammar:
    ///
    ///     AssocConst = "const" Ident ":" Ty "=" Expr ";"
    fn parse_assoc_const(&mut self) -> PResult<'a, (Ident, AssocItemKind, Generics)> {
        self.expect_keyword(kw::Const)?;
        let ident = self.parse_ident()?;
        self.expect(&token::Colon)?;
        let ty = self.parse_ty()?;
        let expr = if self.eat(&token::Eq) { Some(self.parse_expr()?) } else { None };
        self.expect_semi()?;
        Ok((ident, AssocItemKind::Const(ty, expr), Generics::default()))
    }

    /// Parses the following grammar:
    ///
    ///     AssocTy = Ident ["<"...">"] [":" [GenericBounds]] ["where" ...] ["=" Ty]
    fn parse_assoc_ty(&mut self) -> PResult<'a, (Ident, AssocItemKind, Generics)> {
        let ident = self.parse_ident()?;
        let mut generics = self.parse_generics()?;

        // Parse optional colon and param bounds.
        let bounds =
            if self.eat(&token::Colon) { self.parse_generic_bounds(None)? } else { Vec::new() };
        generics.where_clause = self.parse_where_clause()?;

        let default = if self.eat(&token::Eq) { Some(self.parse_ty()?) } else { None };
        self.expect_semi()?;

        Ok((ident, AssocItemKind::TyAlias(bounds, default), generics))
    }

    /// Parses a `UseTree`.
    ///
    /// ```
    /// USE_TREE = [`::`] `*` |
    ///            [`::`] `{` USE_TREE_LIST `}` |
    ///            PATH `::` `*` |
    ///            PATH `::` `{` USE_TREE_LIST `}` |
    ///            PATH [`as` IDENT]
    /// ```
    fn parse_use_tree(&mut self) -> PResult<'a, UseTree> {
        let lo = self.token.span;

        let mut prefix = ast::Path { segments: Vec::new(), span: lo.shrink_to_lo() };
        let kind = if self.check(&token::OpenDelim(token::Brace))
            || self.check(&token::BinOp(token::Star))
            || self.is_import_coupler()
        {
            // `use *;` or `use ::*;` or `use {...};` or `use ::{...};`
            let mod_sep_ctxt = self.token.span.ctxt();
            if self.eat(&token::ModSep) {
                prefix
                    .segments
                    .push(PathSegment::path_root(lo.shrink_to_lo().with_ctxt(mod_sep_ctxt)));
            }

            self.parse_use_tree_glob_or_nested()?
        } else {
            // `use path::*;` or `use path::{...};` or `use path;` or `use path as bar;`
            prefix = self.parse_path(PathStyle::Mod)?;

            if self.eat(&token::ModSep) {
                self.parse_use_tree_glob_or_nested()?
            } else {
                UseTreeKind::Simple(self.parse_rename()?, DUMMY_NODE_ID, DUMMY_NODE_ID)
            }
        };

        Ok(UseTree { prefix, kind, span: lo.to(self.prev_span) })
    }

    /// Parses `*` or `{...}`.
    fn parse_use_tree_glob_or_nested(&mut self) -> PResult<'a, UseTreeKind> {
        Ok(if self.eat(&token::BinOp(token::Star)) {
            UseTreeKind::Glob
        } else {
            UseTreeKind::Nested(self.parse_use_tree_list()?)
        })
    }

    /// Parses a `UseTreeKind::Nested(list)`.
    ///
    /// ```
    /// USE_TREE_LIST = Ø | (USE_TREE `,`)* USE_TREE [`,`]
    /// ```
    fn parse_use_tree_list(&mut self) -> PResult<'a, Vec<(UseTree, ast::NodeId)>> {
        self.parse_delim_comma_seq(token::Brace, |p| Ok((p.parse_use_tree()?, DUMMY_NODE_ID)))
            .map(|(r, _)| r)
    }

    fn parse_rename(&mut self) -> PResult<'a, Option<Ident>> {
        if self.eat_keyword(kw::As) { self.parse_ident_or_underscore().map(Some) } else { Ok(None) }
    }

    fn parse_ident_or_underscore(&mut self) -> PResult<'a, ast::Ident> {
        match self.token.kind {
            token::Ident(name, false) if name == kw::Underscore => {
                let span = self.token.span;
                self.bump();
                Ok(Ident::new(name, span))
            }
            _ => self.parse_ident(),
        }
    }

    /// Parses `extern crate` links.
    ///
    /// # Examples
    ///
    /// ```
    /// extern crate foo;
    /// extern crate bar as foo;
    /// ```
    fn parse_item_extern_crate(
        &mut self,
        lo: Span,
        visibility: Visibility,
        attrs: Vec<Attribute>,
    ) -> PResult<'a, P<Item>> {
        // Accept `extern crate name-like-this` for better diagnostics
        let orig_name = self.parse_crate_name_with_dashes()?;
        let (item_name, orig_name) = if let Some(rename) = self.parse_rename()? {
            (rename, Some(orig_name.name))
        } else {
            (orig_name, None)
        };
        self.expect_semi()?;

        let span = lo.to(self.prev_span);
        Ok(self.mk_item(span, item_name, ItemKind::ExternCrate(orig_name), visibility, attrs))
    }

    fn parse_crate_name_with_dashes(&mut self) -> PResult<'a, ast::Ident> {
        let error_msg = "crate name using dashes are not valid in `extern crate` statements";
        let suggestion_msg = "if the original crate name uses dashes you need to use underscores \
                              in the code";
        let mut ident = if self.token.is_keyword(kw::SelfLower) {
            self.parse_path_segment_ident()
        } else {
            self.parse_ident()
        }?;
        let mut idents = vec![];
        let mut replacement = vec![];
        let mut fixed_crate_name = false;
        // Accept `extern crate name-like-this` for better diagnostics.
        let dash = token::BinOp(token::BinOpToken::Minus);
        if self.token == dash {
            // Do not include `-` as part of the expected tokens list.
            while self.eat(&dash) {
                fixed_crate_name = true;
                replacement.push((self.prev_span, "_".to_string()));
                idents.push(self.parse_ident()?);
            }
        }
        if fixed_crate_name {
            let fixed_name_sp = ident.span.to(idents.last().unwrap().span);
            let mut fixed_name = format!("{}", ident.name);
            for part in idents {
                fixed_name.push_str(&format!("_{}", part.name));
            }
            ident = Ident::from_str_and_span(&fixed_name, fixed_name_sp);

            self.struct_span_err(fixed_name_sp, error_msg)
                .span_label(fixed_name_sp, "dash-separated idents are not valid")
                .multipart_suggestion(suggestion_msg, replacement, Applicability::MachineApplicable)
                .emit();
        }
        Ok(ident)
    }

    /// Parses `extern` for foreign ABIs modules.
    ///
    /// `extern` is expected to have been
    /// consumed before calling this method.
    ///
    /// # Examples
    ///
    /// ```ignore (only-for-syntax-highlight)
    /// extern "C" {}
    /// extern {}
    /// ```
    fn parse_item_foreign_mod(
        &mut self,
        lo: Span,
        abi: Option<StrLit>,
        visibility: Visibility,
        mut attrs: Vec<Attribute>,
    ) -> PResult<'a, P<Item>> {
        self.expect(&token::OpenDelim(token::Brace))?;

        attrs.extend(self.parse_inner_attributes()?);

        let mut foreign_items = vec![];
        while !self.eat(&token::CloseDelim(token::Brace)) {
            foreign_items.push(self.parse_foreign_item()?);
        }

        let prev_span = self.prev_span;
        let m = ast::ForeignMod { abi, items: foreign_items };
        let invalid = Ident::invalid();
        Ok(self.mk_item(lo.to(prev_span), invalid, ItemKind::ForeignMod(m), visibility, attrs))
    }

    /// Parses a foreign item.
    pub fn parse_foreign_item(&mut self) -> PResult<'a, P<ForeignItem>> {
        maybe_whole!(self, NtForeignItem, |ni| ni);

        let mut attrs = self.parse_outer_attributes()?;
        let lo = self.token.span;
        let vis = self.parse_visibility(FollowedByType::No)?;

        if self.check_keyword(kw::Type) {
            // FOREIGN TYPE ITEM
            self.parse_item_foreign_type(vis, lo, attrs)
        } else if self.check_fn_front_matter() {
            // FOREIGN FUNCTION ITEM
            let (ident, sig, generics, body) = self.parse_fn(&mut false, &mut attrs, |_| true)?;
            let kind = ForeignItemKind::Fn(sig, generics, body);
            let span = lo.to(self.prev_span);
            Ok(P(ast::ForeignItem {
                ident,
                attrs,
                kind,
                id: DUMMY_NODE_ID,
                span,
                vis,
                tokens: None,
            }))
        } else if self.is_static_global() {
            // FOREIGN STATIC ITEM
            self.bump(); // `static`
            self.parse_item_foreign_static(vis, lo, attrs)
        } else if self.token.is_keyword(kw::Const) {
            // Treat `const` as `static` for error recovery, but don't add it to expected tokens.
            self.bump(); // `const`
            self.struct_span_err(self.prev_span, "extern items cannot be `const`")
                .span_suggestion(
                    self.prev_span,
                    "try using a static value",
                    "static".to_owned(),
                    Applicability::MachineApplicable,
                )
                .emit();
            self.parse_item_foreign_static(vis, lo, attrs)
        } else if let Some(mac) = self.parse_assoc_macro_invoc("extern", Some(&vis), &mut false)? {
            let kind = ForeignItemKind::Macro(mac);
            let span = lo.to(self.prev_span);
            let ident = Ident::invalid();
            Ok(P(ForeignItem { ident, span, id: DUMMY_NODE_ID, attrs, vis, kind, tokens: None }))
        } else {
            if !attrs.is_empty() {
                self.expected_item_err(&attrs)?;
            }
            self.unexpected()
        }
    }

    /// Parses a static item from a foreign module.
    /// Assumes that the `static` keyword is already parsed.
    fn parse_item_foreign_static(
        &mut self,
        vis: ast::Visibility,
        lo: Span,
        attrs: Vec<Attribute>,
    ) -> PResult<'a, P<ForeignItem>> {
        let mutbl = self.parse_mutability();
        let ident = self.parse_ident()?;
        self.expect(&token::Colon)?;
        let ty = self.parse_ty()?;
        let hi = self.token.span;
        self.expect_semi()?;
        Ok(P(ForeignItem {
            ident,
            attrs,
            kind: ForeignItemKind::Static(ty, mutbl),
            id: DUMMY_NODE_ID,
            span: lo.to(hi),
            vis,
            tokens: None,
        }))
    }

    /// Parses a type from a foreign module.
    fn parse_item_foreign_type(
        &mut self,
        vis: ast::Visibility,
        lo: Span,
        attrs: Vec<Attribute>,
    ) -> PResult<'a, P<ForeignItem>> {
        self.expect_keyword(kw::Type)?;

        let ident = self.parse_ident()?;
        let hi = self.token.span;
        self.expect_semi()?;
        Ok(P(ast::ForeignItem {
            ident,
            attrs,
            kind: ForeignItemKind::Ty,
            id: DUMMY_NODE_ID,
            span: lo.to(hi),
            vis,
            tokens: None,
        }))
    }

    fn is_static_global(&mut self) -> bool {
        if self.check_keyword(kw::Static) {
            // Check if this could be a closure.
            !self.look_ahead(1, |token| {
                if token.is_keyword(kw::Move) {
                    return true;
                }
                match token.kind {
                    token::BinOp(token::Or) | token::OrOr => true,
                    _ => false,
                }
            })
        } else {
            false
        }
    }

    /// Parse `["const" | ("static" "mut"?)] $ident ":" $ty = $expr` with
    /// `["const" | ("static" "mut"?)]` already parsed and stored in `m`.
    ///
    /// When `m` is `"const"`, `$ident` may also be `"_"`.
    fn parse_item_const(&mut self, m: Option<Mutability>) -> PResult<'a, ItemInfo> {
        let id = if m.is_none() { self.parse_ident_or_underscore() } else { self.parse_ident() }?;

        // Parse the type of a `const` or `static mut?` item.
        // That is, the `":" $ty` fragment.
        let ty = if self.token == token::Eq {
            self.recover_missing_const_type(id, m)
        } else {
            // Not `=` so expect `":"" $ty` as usual.
            self.expect(&token::Colon)?;
            self.parse_ty()?
        };

        self.expect(&token::Eq)?;
        let e = self.parse_expr()?;
        self.expect_semi()?;
        let item = match m {
            Some(m) => ItemKind::Static(ty, m, e),
            None => ItemKind::Const(ty, e),
        };
        Ok((id, item, None))
    }

    /// We were supposed to parse `:` but instead, we're already at `=`.
    /// This means that the type is missing.
    fn recover_missing_const_type(&mut self, id: Ident, m: Option<Mutability>) -> P<Ty> {
        // Construct the error and stash it away with the hope
        // that typeck will later enrich the error with a type.
        let kind = match m {
            Some(Mutability::Mut) => "static mut",
            Some(Mutability::Not) => "static",
            None => "const",
        };
        let mut err = self.struct_span_err(id.span, &format!("missing type for `{}` item", kind));
        err.span_suggestion(
            id.span,
            "provide a type for the item",
            format!("{}: <type>", id),
            Applicability::HasPlaceholders,
        );
        err.stash(id.span, StashKey::ItemNoType);

        // The user intended that the type be inferred,
        // so treat this as if the user wrote e.g. `const A: _ = expr;`.
        P(Ty { kind: TyKind::Infer, span: id.span, id: ast::DUMMY_NODE_ID })
    }

    /// Parses the grammar:
    ///     Ident ["<"...">"] ["where" ...] ("=" | ":") Ty ";"
    fn parse_type_alias(&mut self) -> PResult<'a, (Ident, P<Ty>, Generics)> {
        let ident = self.parse_ident()?;
        let mut tps = self.parse_generics()?;
        tps.where_clause = self.parse_where_clause()?;
        self.expect(&token::Eq)?;
        let ty = self.parse_ty()?;
        self.expect_semi()?;
        Ok((ident, ty, tps))
    }

    /// Parses an enum declaration.
    fn parse_item_enum(&mut self) -> PResult<'a, ItemInfo> {
        let id = self.parse_ident()?;
        let mut generics = self.parse_generics()?;
        generics.where_clause = self.parse_where_clause()?;

        let (variants, _) =
            self.parse_delim_comma_seq(token::Brace, |p| p.parse_enum_variant()).map_err(|e| {
                self.recover_stmt();
                e
            })?;

        let enum_definition =
            EnumDef { variants: variants.into_iter().filter_map(|v| v).collect() };
        Ok((id, ItemKind::Enum(enum_definition, generics), None))
    }

    fn parse_enum_variant(&mut self) -> PResult<'a, Option<Variant>> {
        let variant_attrs = self.parse_outer_attributes()?;
        let vlo = self.token.span;

        let vis = self.parse_visibility(FollowedByType::No)?;
        if !self.recover_nested_adt_item(kw::Enum)? {
            return Ok(None);
        }
        let ident = self.parse_ident()?;

        let struct_def = if self.check(&token::OpenDelim(token::Brace)) {
            // Parse a struct variant.
            let (fields, recovered) = self.parse_record_struct_body()?;
            VariantData::Struct(fields, recovered)
        } else if self.check(&token::OpenDelim(token::Paren)) {
            VariantData::Tuple(self.parse_tuple_struct_body()?, DUMMY_NODE_ID)
        } else {
            VariantData::Unit(DUMMY_NODE_ID)
        };

        let disr_expr =
            if self.eat(&token::Eq) { Some(self.parse_anon_const_expr()?) } else { None };

        let vr = ast::Variant {
            ident,
            vis,
            id: DUMMY_NODE_ID,
            attrs: variant_attrs,
            data: struct_def,
            disr_expr,
            span: vlo.to(self.prev_span),
            is_placeholder: false,
        };

        Ok(Some(vr))
    }

    /// Parses `struct Foo { ... }`.
    fn parse_item_struct(&mut self) -> PResult<'a, ItemInfo> {
        let class_name = self.parse_ident()?;

        let mut generics = self.parse_generics()?;

        // There is a special case worth noting here, as reported in issue #17904.
        // If we are parsing a tuple struct it is the case that the where clause
        // should follow the field list. Like so:
        //
        // struct Foo<T>(T) where T: Copy;
        //
        // If we are parsing a normal record-style struct it is the case
        // that the where clause comes before the body, and after the generics.
        // So if we look ahead and see a brace or a where-clause we begin
        // parsing a record style struct.
        //
        // Otherwise if we look ahead and see a paren we parse a tuple-style
        // struct.

        let vdata = if self.token.is_keyword(kw::Where) {
            generics.where_clause = self.parse_where_clause()?;
            if self.eat(&token::Semi) {
                // If we see a: `struct Foo<T> where T: Copy;` style decl.
                VariantData::Unit(DUMMY_NODE_ID)
            } else {
                // If we see: `struct Foo<T> where T: Copy { ... }`
                let (fields, recovered) = self.parse_record_struct_body()?;
                VariantData::Struct(fields, recovered)
            }
        // No `where` so: `struct Foo<T>;`
        } else if self.eat(&token::Semi) {
            VariantData::Unit(DUMMY_NODE_ID)
        // Record-style struct definition
        } else if self.token == token::OpenDelim(token::Brace) {
            let (fields, recovered) = self.parse_record_struct_body()?;
            VariantData::Struct(fields, recovered)
        // Tuple-style struct definition with optional where-clause.
        } else if self.token == token::OpenDelim(token::Paren) {
            let body = VariantData::Tuple(self.parse_tuple_struct_body()?, DUMMY_NODE_ID);
            generics.where_clause = self.parse_where_clause()?;
            self.expect_semi()?;
            body
        } else {
            let token_str = super::token_descr(&self.token);
            let msg = &format!(
                "expected `where`, `{{`, `(`, or `;` after struct name, found {}",
                token_str
            );
            let mut err = self.struct_span_err(self.token.span, msg);
            err.span_label(self.token.span, "expected `where`, `{`, `(`, or `;` after struct name");
            return Err(err);
        };

        Ok((class_name, ItemKind::Struct(vdata, generics), None))
    }

    /// Parses `union Foo { ... }`.
    fn parse_item_union(&mut self) -> PResult<'a, ItemInfo> {
        let class_name = self.parse_ident()?;

        let mut generics = self.parse_generics()?;

        let vdata = if self.token.is_keyword(kw::Where) {
            generics.where_clause = self.parse_where_clause()?;
            let (fields, recovered) = self.parse_record_struct_body()?;
            VariantData::Struct(fields, recovered)
        } else if self.token == token::OpenDelim(token::Brace) {
            let (fields, recovered) = self.parse_record_struct_body()?;
            VariantData::Struct(fields, recovered)
        } else {
            let token_str = super::token_descr(&self.token);
            let msg = &format!("expected `where` or `{{` after union name, found {}", token_str);
            let mut err = self.struct_span_err(self.token.span, msg);
            err.span_label(self.token.span, "expected `where` or `{` after union name");
            return Err(err);
        };

        Ok((class_name, ItemKind::Union(vdata, generics), None))
    }

    pub(super) fn is_union_item(&self) -> bool {
        self.token.is_keyword(kw::Union)
            && self.look_ahead(1, |t| t.is_ident() && !t.is_reserved_ident())
    }

    fn parse_record_struct_body(
        &mut self,
    ) -> PResult<'a, (Vec<StructField>, /* recovered */ bool)> {
        let mut fields = Vec::new();
        let mut recovered = false;
        if self.eat(&token::OpenDelim(token::Brace)) {
            while self.token != token::CloseDelim(token::Brace) {
                let field = self.parse_struct_decl_field().map_err(|e| {
                    self.consume_block(token::Brace, ConsumeClosingDelim::No);
                    recovered = true;
                    e
                });
                match field {
                    Ok(field) => fields.push(field),
                    Err(mut err) => {
                        err.emit();
                        break;
                    }
                }
            }
            self.eat(&token::CloseDelim(token::Brace));
        } else {
            let token_str = super::token_descr(&self.token);
            let msg = &format!("expected `where`, or `{{` after struct name, found {}", token_str);
            let mut err = self.struct_span_err(self.token.span, msg);
            err.span_label(self.token.span, "expected `where`, or `{` after struct name");
            return Err(err);
        }

        Ok((fields, recovered))
    }

    fn parse_tuple_struct_body(&mut self) -> PResult<'a, Vec<StructField>> {
        // This is the case where we find `struct Foo<T>(T) where T: Copy;`
        // Unit like structs are handled in parse_item_struct function
        self.parse_paren_comma_seq(|p| {
            let attrs = p.parse_outer_attributes()?;
            let lo = p.token.span;
            let vis = p.parse_visibility(FollowedByType::Yes)?;
            let ty = p.parse_ty()?;
            Ok(StructField {
                span: lo.to(ty.span),
                vis,
                ident: None,
                id: DUMMY_NODE_ID,
                ty,
                attrs,
                is_placeholder: false,
            })
        })
        .map(|(r, _)| r)
    }

    /// Parses an element of a struct declaration.
    fn parse_struct_decl_field(&mut self) -> PResult<'a, StructField> {
        let attrs = self.parse_outer_attributes()?;
        let lo = self.token.span;
        let vis = self.parse_visibility(FollowedByType::No)?;
        self.parse_single_struct_field(lo, vis, attrs)
    }

    /// Parses a structure field declaration.
    fn parse_single_struct_field(
        &mut self,
        lo: Span,
        vis: Visibility,
        attrs: Vec<Attribute>,
    ) -> PResult<'a, StructField> {
        let mut seen_comma: bool = false;
        let a_var = self.parse_name_and_ty(lo, vis, attrs)?;
        if self.token == token::Comma {
            seen_comma = true;
        }
        match self.token.kind {
            token::Comma => {
                self.bump();
            }
            token::CloseDelim(token::Brace) => {}
            token::DocComment(_) => {
                let previous_span = self.prev_span;
                let mut err = self.span_fatal_err(self.token.span, Error::UselessDocComment);
                self.bump(); // consume the doc comment
                let comma_after_doc_seen = self.eat(&token::Comma);
                // `seen_comma` is always false, because we are inside doc block
                // condition is here to make code more readable
                if seen_comma == false && comma_after_doc_seen == true {
                    seen_comma = true;
                }
                if comma_after_doc_seen || self.token == token::CloseDelim(token::Brace) {
                    err.emit();
                } else {
                    if seen_comma == false {
                        let sp = self.sess.source_map().next_point(previous_span);
                        err.span_suggestion(
                            sp,
                            "missing comma here",
                            ",".into(),
                            Applicability::MachineApplicable,
                        );
                    }
                    return Err(err);
                }
            }
            _ => {
                let sp = self.prev_span.shrink_to_hi();
                let mut err = self.struct_span_err(
                    sp,
                    &format!("expected `,`, or `}}`, found {}", super::token_descr(&self.token)),
                );
                if self.token.is_ident() {
                    // This is likely another field; emit the diagnostic and keep going
                    err.span_suggestion(
                        sp,
                        "try adding a comma",
                        ",".into(),
                        Applicability::MachineApplicable,
                    );
                    err.emit();
                } else {
                    return Err(err);
                }
            }
        }
        Ok(a_var)
    }

    /// Parses a structure field.
    fn parse_name_and_ty(
        &mut self,
        lo: Span,
        vis: Visibility,
        attrs: Vec<Attribute>,
    ) -> PResult<'a, StructField> {
        let name = self.parse_ident()?;
        self.expect(&token::Colon)?;
        let ty = self.parse_ty()?;
        Ok(StructField {
            span: lo.to(self.prev_span),
            ident: Some(name),
            vis,
            id: DUMMY_NODE_ID,
            ty,
            attrs,
            is_placeholder: false,
        })
    }

    pub(super) fn eat_macro_def(
        &mut self,
        attrs: &[Attribute],
        vis: &Visibility,
        lo: Span,
    ) -> PResult<'a, Option<P<Item>>> {
        let (ident, def) = if self.eat_keyword(kw::Macro) {
            let ident = self.parse_ident()?;
            let body = if self.check(&token::OpenDelim(token::Brace)) {
                self.parse_mac_args()?
            } else if self.check(&token::OpenDelim(token::Paren)) {
                let params = self.parse_token_tree();
                let pspan = params.span();
                let body = if self.check(&token::OpenDelim(token::Brace)) {
                    self.parse_token_tree()
                } else {
                    return self.unexpected();
                };
                let bspan = body.span();
                let tokens = TokenStream::new(vec![
                    params.into(),
                    TokenTree::token(token::FatArrow, pspan.between(bspan)).into(),
                    body.into(),
                ]);
                let dspan = DelimSpan::from_pair(pspan.shrink_to_lo(), bspan.shrink_to_hi());
                P(MacArgs::Delimited(dspan, MacDelimiter::Brace, tokens))
            } else {
                return self.unexpected();
            };

            (ident, ast::MacroDef { body, legacy: false })
        } else if self.check_keyword(sym::macro_rules)
            && self.look_ahead(1, |t| *t == token::Not)
            && self.look_ahead(2, |t| t.is_ident())
        {
            let prev_span = self.prev_span;
            self.complain_if_pub_macro(&vis.node, prev_span);
            self.bump();
            self.bump();

            let ident = self.parse_ident()?;
            let body = self.parse_mac_args()?;
            if body.need_semicolon() && !self.eat(&token::Semi) {
                self.report_invalid_macro_expansion_item();
            }

            (ident, ast::MacroDef { body, legacy: true })
        } else {
            return Ok(None);
        };

        let span = lo.to(self.prev_span);

        if !def.legacy {
            self.sess.gated_spans.gate(sym::decl_macro, span);
        }

        Ok(Some(self.mk_item(span, ident, ItemKind::MacroDef(def), vis.clone(), attrs.to_vec())))
    }

    fn complain_if_pub_macro(&self, vis: &VisibilityKind, sp: Span) {
        match *vis {
            VisibilityKind::Inherited => {}
            _ => {
                let mut err = if self.token.is_keyword(sym::macro_rules) {
                    let mut err =
                        self.struct_span_err(sp, "can't qualify macro_rules invocation with `pub`");
                    err.span_suggestion(
                        sp,
                        "try exporting the macro",
                        "#[macro_export]".to_owned(),
                        Applicability::MaybeIncorrect, // speculative
                    );
                    err
                } else {
                    let mut err =
                        self.struct_span_err(sp, "can't qualify macro invocation with `pub`");
                    err.help("try adjusting the macro to put `pub` inside the invocation");
                    err
                };
                err.emit();
            }
        }
    }

    fn report_invalid_macro_expansion_item(&self) {
        let has_close_delim = self
            .sess
            .source_map()
            .span_to_snippet(self.prev_span)
            .map(|s| s.ends_with(")") || s.ends_with("]"))
            .unwrap_or(false);

        let mut err = self.struct_span_err(
            self.prev_span,
            "macros that expand to items must be delimited with braces or followed by a semicolon",
        );

        // To avoid ICE, we shouldn't emit actual suggestions when it hasn't closing delims
        if has_close_delim {
            err.multipart_suggestion(
                "change the delimiters to curly braces",
                vec![
                    (self.prev_span.with_hi(self.prev_span.lo() + BytePos(1)), '{'.to_string()),
                    (self.prev_span.with_lo(self.prev_span.hi() - BytePos(1)), '}'.to_string()),
                ],
                Applicability::MaybeIncorrect,
            );
        } else {
            err.span_suggestion(
                self.prev_span,
                "change the delimiters to curly braces",
                " { /* items */ }".to_string(),
                Applicability::HasPlaceholders,
            );
        }

        err.span_suggestion(
            self.prev_span.shrink_to_hi(),
            "add a semicolon",
            ';'.to_string(),
            Applicability::MaybeIncorrect,
        )
        .emit();
    }

    /// Checks if current token is one of tokens which cannot be nested like `kw::Enum`. In case
    /// it is, we try to parse the item and report error about nested types.
    fn recover_nested_adt_item(&mut self, keyword: Symbol) -> PResult<'a, bool> {
        if (self.token.is_keyword(kw::Enum)
            || self.token.is_keyword(kw::Struct)
            || self.token.is_keyword(kw::Union))
            && self.look_ahead(1, |t| t.is_ident())
        {
            let kw_token = self.token.clone();
            let kw_str = pprust::token_to_string(&kw_token);
            let item = self.parse_item()?;

            self.struct_span_err(
                kw_token.span,
                &format!("`{}` definition cannot be nested inside `{}`", kw_str, keyword),
            )
            .span_suggestion(
                item.unwrap().span,
                &format!("consider creating a new `{}` definition instead of nesting", kw_str),
                String::new(),
                Applicability::MaybeIncorrect,
            )
            .emit();
            // We successfully parsed the item but we must inform the caller about nested problem.
            return Ok(false);
        }
        Ok(true)
    }

    fn mk_item(
        &self,
        span: Span,
        ident: Ident,
        kind: ItemKind,
        vis: Visibility,
        attrs: Vec<Attribute>,
    ) -> P<Item> {
        P(Item { ident, attrs, id: DUMMY_NODE_ID, kind, vis, span, tokens: None })
    }
}

/// The parsing configuration used to parse a parameter list (see `parse_fn_params`).
///
/// The function decides if, per-parameter `p`, `p` must have a pattern or just a type.
type ReqName = fn(&token::Token) -> bool;

/// Parsing of functions and methods.
impl<'a> Parser<'a> {
    /// Parse a function starting from the front matter (`const ...`) to the body `{ ... }` or `;`.
    fn parse_fn(
        &mut self,
        at_end: &mut bool,
        attrs: &mut Vec<Attribute>,
        req_name: ReqName,
    ) -> PResult<'a, (Ident, FnSig, Generics, Option<P<Block>>)> {
        let header = self.parse_fn_front_matter()?; // `const ... fn`
        let ident = self.parse_ident()?; // `foo`
        let mut generics = self.parse_generics()?; // `<'a, T, ...>`
        let decl = self.parse_fn_decl(req_name, AllowPlus::Yes)?; // `(p: u8, ...)`
        generics.where_clause = self.parse_where_clause()?; // `where T: Ord`
        let body = self.parse_fn_body(at_end, attrs)?; // `;` or `{ ... }`.
        Ok((ident, FnSig { header, decl }, generics, body))
    }

    /// Parse the "body" of a function.
    /// This can either be `;` when there's no body,
    /// or e.g. a block when the function is a provided one.
    fn parse_fn_body(
        &mut self,
        at_end: &mut bool,
        attrs: &mut Vec<Attribute>,
    ) -> PResult<'a, Option<P<Block>>> {
        let (inner_attrs, body) = match self.token.kind {
            token::Semi => {
                self.bump();
                (Vec::new(), None)
            }
            token::OpenDelim(token::Brace) => {
                let (attrs, body) = self.parse_inner_attrs_and_block()?;
                (attrs, Some(body))
            }
            token::Interpolated(ref nt) => match **nt {
                token::NtBlock(..) => {
                    let (attrs, body) = self.parse_inner_attrs_and_block()?;
                    (attrs, Some(body))
                }
                _ => return self.expected_semi_or_open_brace(),
            },
            _ => return self.expected_semi_or_open_brace(),
        };
        attrs.extend(inner_attrs);
        *at_end = true;
        Ok(body)
    }

    /// Is the current token the start of an `FnHeader` / not a valid parse?
    fn check_fn_front_matter(&mut self) -> bool {
        // We use an over-approximation here.
        // `const const`, `fn const` won't parse, but we're not stepping over other syntax either.
        const QUALS: [Symbol; 4] = [kw::Const, kw::Async, kw::Unsafe, kw::Extern];
        self.check_keyword(kw::Fn) // Definitely an `fn`.
            // `$qual fn` or `$qual $qual`:
            || QUALS.iter().any(|&kw| self.check_keyword(kw))
                && self.look_ahead(1, |t| {
                    // ...qualified and then `fn`, e.g. `const fn`.
                    t.is_keyword(kw::Fn)
                    // Two qualifiers. This is enough. Due `async` we need to check that it's reserved.
                    || t.is_non_raw_ident_where(|i| QUALS.contains(&i.name) && i.is_reserved())
                })
            // `extern ABI fn`
            || self.check_keyword(kw::Extern)
                && self.look_ahead(1, |t| t.can_begin_literal_or_bool())
                && self.look_ahead(2, |t| t.is_keyword(kw::Fn))
    }

    /// Parses all the "front matter" (or "qualifiers") for a `fn` declaration,
    /// up to and including the `fn` keyword. The formal grammar is:
    ///
    /// ```
    /// Extern = "extern" StringLit ;
    /// FnQual = "const"? "async"? "unsafe"? Extern? ;
    /// FnFrontMatter = FnQual? "fn" ;
    /// ```
    fn parse_fn_front_matter(&mut self) -> PResult<'a, FnHeader> {
        let constness = self.parse_constness();
        let asyncness = self.parse_asyncness();
        let unsafety = self.parse_unsafety();
        let ext = self.parse_extern()?;

        if let Async::Yes { span, .. } = asyncness {
            self.ban_async_in_2015(span);
        }

        if !self.eat_keyword(kw::Fn) {
            // It is possible for `expect_one_of` to recover given the contents of
            // `self.expected_tokens`, therefore, do not use `self.unexpected()` which doesn't
            // account for this.
            if !self.expect_one_of(&[], &[])? {
                unreachable!()
            }
        }

        Ok(FnHeader { constness, unsafety, asyncness, ext })
    }

    /// We are parsing `async fn`. If we are on Rust 2015, emit an error.
    fn ban_async_in_2015(&self, span: Span) {
        if span.rust_2015() {
            let diag = self.diagnostic();
            struct_span_err!(diag, span, E0670, "`async fn` is not permitted in the 2015 edition")
                .note("to use `async fn`, switch to Rust 2018")
                .help("set `edition = \"2018\"` in `Cargo.toml`")
                .note("for more on editions, read https://doc.rust-lang.org/edition-guide")
                .emit();
        }
    }

    /// Parses the parameter list and result type of a function declaration.
    pub(super) fn parse_fn_decl(
        &mut self,
        req_name: ReqName,
        ret_allow_plus: AllowPlus,
    ) -> PResult<'a, P<FnDecl>> {
        Ok(P(FnDecl {
            inputs: self.parse_fn_params(req_name)?,
            output: self.parse_ret_ty(ret_allow_plus, RecoverQPath::Yes)?,
        }))
    }

    /// Parses the parameter list of a function, including the `(` and `)` delimiters.
    fn parse_fn_params(&mut self, req_name: ReqName) -> PResult<'a, Vec<Param>> {
        let mut first_param = true;
        // Parse the arguments, starting out with `self` being allowed...
        let (mut params, _) = self.parse_paren_comma_seq(|p| {
            let param = p.parse_param_general(req_name, first_param).or_else(|mut e| {
                e.emit();
                let lo = p.prev_span;
                // Skip every token until next possible arg or end.
                p.eat_to_tokens(&[&token::Comma, &token::CloseDelim(token::Paren)]);
                // Create a placeholder argument for proper arg count (issue #34264).
                Ok(dummy_arg(Ident::new(kw::Invalid, lo.to(p.prev_span))))
            });
            // ...now that we've parsed the first argument, `self` is no longer allowed.
            first_param = false;
            param
        })?;
        // Replace duplicated recovered params with `_` pattern to avoid unnecessary errors.
        self.deduplicate_recovered_params_names(&mut params);
        Ok(params)
    }

    /// Parses a single function parameter.
    ///
    /// - `self` is syntactically allowed when `first_param` holds.
    fn parse_param_general(&mut self, req_name: ReqName, first_param: bool) -> PResult<'a, Param> {
        let lo = self.token.span;
        let attrs = self.parse_outer_attributes()?;

        // Possibly parse `self`. Recover if we parsed it and it wasn't allowed here.
        if let Some(mut param) = self.parse_self_param()? {
            param.attrs = attrs.into();
            return if first_param { Ok(param) } else { self.recover_bad_self_param(param) };
        }

        let is_name_required = match self.token.kind {
            token::DotDotDot => false,
            _ => req_name(&self.token),
        };
        let (pat, ty) = if is_name_required || self.is_named_param() {
            debug!("parse_param_general parse_pat (is_name_required:{})", is_name_required);

            let pat = self.parse_fn_param_pat()?;
            if let Err(mut err) = self.expect(&token::Colon) {
                return if let Some(ident) =
                    self.parameter_without_type(&mut err, pat, is_name_required, first_param)
                {
                    err.emit();
                    Ok(dummy_arg(ident))
                } else {
                    Err(err)
                };
            }

            self.eat_incorrect_doc_comment_for_param_type();
            (pat, self.parse_ty_for_param()?)
        } else {
            debug!("parse_param_general ident_to_pat");
            let parser_snapshot_before_ty = self.clone();
            self.eat_incorrect_doc_comment_for_param_type();
            let mut ty = self.parse_ty_for_param();
            if ty.is_ok()
                && self.token != token::Comma
                && self.token != token::CloseDelim(token::Paren)
            {
                // This wasn't actually a type, but a pattern looking like a type,
                // so we are going to rollback and re-parse for recovery.
                ty = self.unexpected();
            }
            match ty {
                Ok(ty) => {
                    let ident = Ident::new(kw::Invalid, self.prev_span);
                    let bm = BindingMode::ByValue(Mutability::Not);
                    let pat = self.mk_pat_ident(ty.span, bm, ident);
                    (pat, ty)
                }
                // If this is a C-variadic argument and we hit an error, return the error.
                Err(err) if self.token == token::DotDotDot => return Err(err),
                // Recover from attempting to parse the argument as a type without pattern.
                Err(mut err) => {
                    err.cancel();
                    mem::replace(self, parser_snapshot_before_ty);
                    self.recover_arg_parse()?
                }
            }
        };

        let span = lo.to(self.token.span);

        Ok(Param {
            attrs: attrs.into(),
            id: ast::DUMMY_NODE_ID,
            is_placeholder: false,
            pat,
            span,
            ty,
        })
    }

    /// Returns the parsed optional self parameter and whether a self shortcut was used.
    fn parse_self_param(&mut self) -> PResult<'a, Option<Param>> {
        // Extract an identifier *after* having confirmed that the token is one.
        let expect_self_ident = |this: &mut Self| {
            match this.token.kind {
                // Preserve hygienic context.
                token::Ident(name, _) => {
                    let span = this.token.span;
                    this.bump();
                    Ident::new(name, span)
                }
                _ => unreachable!(),
            }
        };
        // Is `self` `n` tokens ahead?
        let is_isolated_self = |this: &Self, n| {
            this.is_keyword_ahead(n, &[kw::SelfLower])
                && this.look_ahead(n + 1, |t| t != &token::ModSep)
        };
        // Is `mut self` `n` tokens ahead?
        let is_isolated_mut_self =
            |this: &Self, n| this.is_keyword_ahead(n, &[kw::Mut]) && is_isolated_self(this, n + 1);
        // Parse `self` or `self: TYPE`. We already know the current token is `self`.
        let parse_self_possibly_typed = |this: &mut Self, m| {
            let eself_ident = expect_self_ident(this);
            let eself_hi = this.prev_span;
            let eself = if this.eat(&token::Colon) {
                SelfKind::Explicit(this.parse_ty()?, m)
            } else {
                SelfKind::Value(m)
            };
            Ok((eself, eself_ident, eself_hi))
        };
        // Recover for the grammar `*self`, `*const self`, and `*mut self`.
        let recover_self_ptr = |this: &mut Self| {
            let msg = "cannot pass `self` by raw pointer";
            let span = this.token.span;
            this.struct_span_err(span, msg).span_label(span, msg).emit();

            Ok((SelfKind::Value(Mutability::Not), expect_self_ident(this), this.prev_span))
        };

        // Parse optional `self` parameter of a method.
        // Only a limited set of initial token sequences is considered `self` parameters; anything
        // else is parsed as a normal function parameter list, so some lookahead is required.
        let eself_lo = self.token.span;
        let (eself, eself_ident, eself_hi) = match self.token.kind {
            token::BinOp(token::And) => {
                let eself = if is_isolated_self(self, 1) {
                    // `&self`
                    self.bump();
                    SelfKind::Region(None, Mutability::Not)
                } else if is_isolated_mut_self(self, 1) {
                    // `&mut self`
                    self.bump();
                    self.bump();
                    SelfKind::Region(None, Mutability::Mut)
                } else if self.look_ahead(1, |t| t.is_lifetime()) && is_isolated_self(self, 2) {
                    // `&'lt self`
                    self.bump();
                    let lt = self.expect_lifetime();
                    SelfKind::Region(Some(lt), Mutability::Not)
                } else if self.look_ahead(1, |t| t.is_lifetime()) && is_isolated_mut_self(self, 2) {
                    // `&'lt mut self`
                    self.bump();
                    let lt = self.expect_lifetime();
                    self.bump();
                    SelfKind::Region(Some(lt), Mutability::Mut)
                } else {
                    // `&not_self`
                    return Ok(None);
                };
                (eself, expect_self_ident(self), self.prev_span)
            }
            // `*self`
            token::BinOp(token::Star) if is_isolated_self(self, 1) => {
                self.bump();
                recover_self_ptr(self)?
            }
            // `*mut self` and `*const self`
            token::BinOp(token::Star)
                if self.look_ahead(1, |t| t.is_mutability()) && is_isolated_self(self, 2) =>
            {
                self.bump();
                self.bump();
                recover_self_ptr(self)?
            }
            // `self` and `self: TYPE`
            token::Ident(..) if is_isolated_self(self, 0) => {
                parse_self_possibly_typed(self, Mutability::Not)?
            }
            // `mut self` and `mut self: TYPE`
            token::Ident(..) if is_isolated_mut_self(self, 0) => {
                self.bump();
                parse_self_possibly_typed(self, Mutability::Mut)?
            }
            _ => return Ok(None),
        };

        let eself = source_map::respan(eself_lo.to(eself_hi), eself);
        Ok(Some(Param::from_self(AttrVec::default(), eself, eself_ident)))
    }

    fn is_named_param(&self) -> bool {
        let offset = match self.token.kind {
            token::Interpolated(ref nt) => match **nt {
                token::NtPat(..) => return self.look_ahead(1, |t| t == &token::Colon),
                _ => 0,
            },
            token::BinOp(token::And) | token::AndAnd => 1,
            _ if self.token.is_keyword(kw::Mut) => 1,
            _ => 0,
        };

        self.look_ahead(offset, |t| t.is_ident())
            && self.look_ahead(offset + 1, |t| t == &token::Colon)
    }

    fn recover_first_param(&mut self) -> &'static str {
        match self
            .parse_outer_attributes()
            .and_then(|_| self.parse_self_param())
            .map_err(|mut e| e.cancel())
        {
            Ok(Some(_)) => "method",
            _ => "function",
        }
    }
}
