// Copyright 2012-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![crate_name = "rustc_resolve"]
#![crate_type = "dylib"]
#![crate_type = "rlib"]
#![doc(html_logo_url = "https://www.rust-lang.org/logos/rust-logo-128x128-blk-v2.png",
      html_favicon_url = "https://doc.rust-lang.org/favicon.ico",
      html_root_url = "https://doc.rust-lang.org/nightly/")]
#![deny(warnings)]

#![feature(rustc_diagnostic_macros)]

#[macro_use]
extern crate log;
#[macro_use]
extern crate syntax;
extern crate syntax_pos;
extern crate rustc_errors as errors;
extern crate arena;
#[macro_use]
extern crate rustc;

use self::Namespace::*;
use self::TypeParameters::*;
use self::RibKind::*;

use rustc::hir::map::{Definitions, DefCollector};
use rustc::hir::{self, PrimTy, TyBool, TyChar, TyFloat, TyInt, TyUint, TyStr};
use rustc::middle::cstore::CrateLoader;
use rustc::session::Session;
use rustc::lint;
use rustc::hir::def::*;
use rustc::hir::def_id::{CRATE_DEF_INDEX, LOCAL_CRATE, DefId};
use rustc::ty;
use rustc::hir::{Freevar, FreevarMap, TraitCandidate, TraitMap, GlobMap};
use rustc::util::nodemap::{NodeMap, NodeSet, FxHashMap, FxHashSet, DefIdMap};

use syntax::codemap::{dummy_spanned, respan};
use syntax::ext::hygiene::{Mark, SyntaxContext};
use syntax::ast::{self, Name, NodeId, Ident, SpannedIdent, FloatTy, IntTy, UintTy};
use syntax::ext::base::SyntaxExtension;
use syntax::ext::base::Determinacy::{self, Determined, Undetermined};
use syntax::ext::base::MacroKind;
use syntax::symbol::{Symbol, keywords};
use syntax::util::lev_distance::find_best_match_for_name;

use syntax::visit::{self, FnKind, Visitor};
use syntax::attr;
use syntax::ast::{Arm, BindingMode, Block, Crate, Expr, ExprKind};
use syntax::ast::{FnDecl, ForeignItem, ForeignItemKind, Generics};
use syntax::ast::{Item, ItemKind, ImplItem, ImplItemKind};
use syntax::ast::{Local, Mutability, Pat, PatKind, Path};
use syntax::ast::{QSelf, TraitItemKind, TraitRef, Ty, TyKind};
use syntax::feature_gate::{feature_err, emit_feature_err, GateIssue};

use syntax_pos::{Span, DUMMY_SP, MultiSpan};
use errors::DiagnosticBuilder;

use std::cell::{Cell, RefCell};
use std::cmp;
use std::collections::BTreeSet;
use std::fmt;
use std::mem::replace;
use std::rc::Rc;

use resolve_imports::{ImportDirective, ImportDirectiveSubclass, NameResolution, ImportResolver};
use macros::{InvocationData, LegacyBinding, LegacyScope, MacroBinding};

// NB: This module needs to be declared first so diagnostics are
// registered before they are used.
mod diagnostics;

mod macros;
mod check_unused;
mod build_reduced_graph;
mod resolve_imports;

/// A free importable items suggested in case of resolution failure.
struct ImportSuggestion {
    path: Path,
}

/// A field or associated item from self type suggested in case of resolution failure.
enum AssocSuggestion {
    Field,
    MethodWithSelf,
    AssocItem,
}

#[derive(Eq)]
struct BindingError {
    name: Name,
    origin: BTreeSet<Span>,
    target: BTreeSet<Span>,
}

impl PartialOrd for BindingError {
    fn partial_cmp(&self, other: &BindingError) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for BindingError {
    fn eq(&self, other: &BindingError) -> bool {
        self.name == other.name
    }
}

impl Ord for BindingError {
    fn cmp(&self, other: &BindingError) -> cmp::Ordering {
        self.name.cmp(&other.name)
    }
}

enum ResolutionError<'a> {
    /// error E0401: can't use type parameters from outer function
    TypeParametersFromOuterFunction,
    /// error E0403: the name is already used for a type parameter in this type parameter list
    NameAlreadyUsedInTypeParameterList(Name, &'a Span),
    /// error E0407: method is not a member of trait
    MethodNotMemberOfTrait(Name, &'a str),
    /// error E0437: type is not a member of trait
    TypeNotMemberOfTrait(Name, &'a str),
    /// error E0438: const is not a member of trait
    ConstNotMemberOfTrait(Name, &'a str),
    /// error E0408: variable `{}` is not bound in all patterns
    VariableNotBoundInPattern(&'a BindingError),
    /// error E0409: variable `{}` is bound in inconsistent ways within the same match arm
    VariableBoundWithDifferentMode(Name, Span),
    /// error E0415: identifier is bound more than once in this parameter list
    IdentifierBoundMoreThanOnceInParameterList(&'a str),
    /// error E0416: identifier is bound more than once in the same pattern
    IdentifierBoundMoreThanOnceInSamePattern(&'a str),
    /// error E0426: use of undeclared label
    UndeclaredLabel(&'a str),
    /// error E0429: `self` imports are only allowed within a { } list
    SelfImportsOnlyAllowedWithin,
    /// error E0430: `self` import can only appear once in the list
    SelfImportCanOnlyAppearOnceInTheList,
    /// error E0431: `self` import can only appear in an import list with a non-empty prefix
    SelfImportOnlyInImportListWithNonEmptyPrefix,
    /// error E0432: unresolved import
    UnresolvedImport(Option<(Span, &'a str, &'a str)>),
    /// error E0433: failed to resolve
    FailedToResolve(&'a str),
    /// error E0434: can't capture dynamic environment in a fn item
    CannotCaptureDynamicEnvironmentInFnItem,
    /// error E0435: attempt to use a non-constant value in a constant
    AttemptToUseNonConstantValueInConstant,
    /// error E0530: X bindings cannot shadow Ys
    BindingShadowsSomethingUnacceptable(&'a str, Name, &'a NameBinding<'a>),
    /// error E0128: type parameters with a default cannot use forward declared identifiers
    ForwardDeclaredTyParam,
}

fn resolve_error<'sess, 'a>(resolver: &'sess Resolver,
                            span: Span,
                            resolution_error: ResolutionError<'a>) {
    resolve_struct_error(resolver, span, resolution_error).emit();
}

fn resolve_struct_error<'sess, 'a>(resolver: &'sess Resolver,
                                   span: Span,
                                   resolution_error: ResolutionError<'a>)
                                   -> DiagnosticBuilder<'sess> {
    match resolution_error {
        ResolutionError::TypeParametersFromOuterFunction => {
            let mut err = struct_span_err!(resolver.session,
                                           span,
                                           E0401,
                                           "can't use type parameters from outer function; \
                                           try using a local type parameter instead");
            err.span_label(span, "use of type variable from outer function");
            err
        }
        ResolutionError::NameAlreadyUsedInTypeParameterList(name, first_use_span) => {
             let mut err = struct_span_err!(resolver.session,
                                            span,
                                            E0403,
                                            "the name `{}` is already used for a type parameter \
                                            in this type parameter list",
                                            name);
             err.span_label(span, "already used");
             err.span_label(first_use_span.clone(), format!("first use of `{}`", name));
             err
        }
        ResolutionError::MethodNotMemberOfTrait(method, trait_) => {
            let mut err = struct_span_err!(resolver.session,
                                           span,
                                           E0407,
                                           "method `{}` is not a member of trait `{}`",
                                           method,
                                           trait_);
            err.span_label(span, format!("not a member of trait `{}`", trait_));
            err
        }
        ResolutionError::TypeNotMemberOfTrait(type_, trait_) => {
            let mut err = struct_span_err!(resolver.session,
                             span,
                             E0437,
                             "type `{}` is not a member of trait `{}`",
                             type_,
                             trait_);
            err.span_label(span, format!("not a member of trait `{}`", trait_));
            err
        }
        ResolutionError::ConstNotMemberOfTrait(const_, trait_) => {
            let mut err = struct_span_err!(resolver.session,
                             span,
                             E0438,
                             "const `{}` is not a member of trait `{}`",
                             const_,
                             trait_);
            err.span_label(span, format!("not a member of trait `{}`", trait_));
            err
        }
        ResolutionError::VariableNotBoundInPattern(binding_error) => {
            let target_sp = binding_error.target.iter().map(|x| *x).collect::<Vec<_>>();
            let msp = MultiSpan::from_spans(target_sp.clone());
            let msg = format!("variable `{}` is not bound in all patterns", binding_error.name);
            let mut err = resolver.session.struct_span_err_with_code(msp, &msg, "E0408");
            for sp in target_sp {
                err.span_label(sp, format!("pattern doesn't bind `{}`", binding_error.name));
            }
            let origin_sp = binding_error.origin.iter().map(|x| *x).collect::<Vec<_>>();
            for sp in origin_sp {
                err.span_label(sp, "variable not in all patterns");
            }
            err
        }
        ResolutionError::VariableBoundWithDifferentMode(variable_name,
                                                        first_binding_span) => {
            let mut err = struct_span_err!(resolver.session,
                             span,
                             E0409,
                             "variable `{}` is bound in inconsistent \
                             ways within the same match arm",
                             variable_name);
            err.span_label(span, "bound in different ways");
            err.span_label(first_binding_span, "first binding");
            err
        }
        ResolutionError::IdentifierBoundMoreThanOnceInParameterList(identifier) => {
            let mut err = struct_span_err!(resolver.session,
                             span,
                             E0415,
                             "identifier `{}` is bound more than once in this parameter list",
                             identifier);
            err.span_label(span, "used as parameter more than once");
            err
        }
        ResolutionError::IdentifierBoundMoreThanOnceInSamePattern(identifier) => {
            let mut err = struct_span_err!(resolver.session,
                             span,
                             E0416,
                             "identifier `{}` is bound more than once in the same pattern",
                             identifier);
            err.span_label(span, "used in a pattern more than once");
            err
        }
        ResolutionError::UndeclaredLabel(name) => {
            let mut err = struct_span_err!(resolver.session,
                                           span,
                                           E0426,
                                           "use of undeclared label `{}`",
                                           name);
            err.span_label(span, format!("undeclared label `{}`", name));
            err
        }
        ResolutionError::SelfImportsOnlyAllowedWithin => {
            struct_span_err!(resolver.session,
                             span,
                             E0429,
                             "{}",
                             "`self` imports are only allowed within a { } list")
        }
        ResolutionError::SelfImportCanOnlyAppearOnceInTheList => {
            struct_span_err!(resolver.session,
                             span,
                             E0430,
                             "`self` import can only appear once in the list")
        }
        ResolutionError::SelfImportOnlyInImportListWithNonEmptyPrefix => {
            struct_span_err!(resolver.session,
                             span,
                             E0431,
                             "`self` import can only appear in an import list with a \
                              non-empty prefix")
        }
        ResolutionError::UnresolvedImport(name) => {
            let (span, msg) = match name {
                Some((sp, n, _)) => (sp, format!("unresolved import `{}`", n)),
                None => (span, "unresolved import".to_owned()),
            };
            let mut err = struct_span_err!(resolver.session, span, E0432, "{}", msg);
            if let Some((_, _, p)) = name {
                err.span_label(span, p);
            }
            err
        }
        ResolutionError::FailedToResolve(msg) => {
            let mut err = struct_span_err!(resolver.session, span, E0433,
                                           "failed to resolve. {}", msg);
            err.span_label(span, msg);
            err
        }
        ResolutionError::CannotCaptureDynamicEnvironmentInFnItem => {
            struct_span_err!(resolver.session,
                             span,
                             E0434,
                             "{}",
                             "can't capture dynamic environment in a fn item; use the || { ... } \
                              closure form instead")
        }
        ResolutionError::AttemptToUseNonConstantValueInConstant => {
            let mut err = struct_span_err!(resolver.session,
                             span,
                             E0435,
                             "attempt to use a non-constant value in a constant");
            err.span_label(span, "non-constant value");
            err
        }
        ResolutionError::BindingShadowsSomethingUnacceptable(what_binding, name, binding) => {
            let shadows_what = PathResolution::new(binding.def()).kind_name();
            let mut err = struct_span_err!(resolver.session,
                                           span,
                                           E0530,
                                           "{}s cannot shadow {}s", what_binding, shadows_what);
            err.span_label(span, format!("cannot be named the same as a {}", shadows_what));
            let participle = if binding.is_import() { "imported" } else { "defined" };
            let msg = format!("a {} `{}` is {} here", shadows_what, name, participle);
            err.span_label(binding.span, msg);
            err
        }
        ResolutionError::ForwardDeclaredTyParam => {
            let mut err = struct_span_err!(resolver.session, span, E0128,
                                           "type parameters with a default cannot use \
                                            forward declared identifiers");
            err.span_label(span, format!("defaulted type parameters \
                                           cannot be forward declared"));
            err
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct BindingInfo {
    span: Span,
    binding_mode: BindingMode,
}

// Map from the name in a pattern to its binding mode.
type BindingMap = FxHashMap<Ident, BindingInfo>;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum PatternSource {
    Match,
    IfLet,
    WhileLet,
    Let,
    For,
    FnParam,
}

impl PatternSource {
    fn is_refutable(self) -> bool {
        match self {
            PatternSource::Match | PatternSource::IfLet | PatternSource::WhileLet => true,
            PatternSource::Let | PatternSource::For | PatternSource::FnParam  => false,
        }
    }
    fn descr(self) -> &'static str {
        match self {
            PatternSource::Match => "match binding",
            PatternSource::IfLet => "if let binding",
            PatternSource::WhileLet => "while let binding",
            PatternSource::Let => "let binding",
            PatternSource::For => "for binding",
            PatternSource::FnParam => "function parameter",
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum PathSource<'a> {
    // Type paths `Path`.
    Type,
    // Trait paths in bounds or impls.
    Trait,
    // Expression paths `path`, with optional parent context.
    Expr(Option<&'a Expr>),
    // Paths in path patterns `Path`.
    Pat,
    // Paths in struct expressions and patterns `Path { .. }`.
    Struct,
    // Paths in tuple struct patterns `Path(..)`.
    TupleStruct,
    // `m::A::B` in `<T as m::A>::B::C`.
    TraitItem(Namespace),
    // Path in `pub(path)`
    Visibility,
    // Path in `use a::b::{...};`
    ImportPrefix,
}

impl<'a> PathSource<'a> {
    fn namespace(self) -> Namespace {
        match self {
            PathSource::Type | PathSource::Trait | PathSource::Struct |
            PathSource::Visibility | PathSource::ImportPrefix => TypeNS,
            PathSource::Expr(..) | PathSource::Pat | PathSource::TupleStruct => ValueNS,
            PathSource::TraitItem(ns) => ns,
        }
    }

    fn global_by_default(self) -> bool {
        match self {
            PathSource::Visibility | PathSource::ImportPrefix => true,
            PathSource::Type | PathSource::Expr(..) | PathSource::Pat |
            PathSource::Struct | PathSource::TupleStruct |
            PathSource::Trait | PathSource::TraitItem(..) => false,
        }
    }

    fn defer_to_typeck(self) -> bool {
        match self {
            PathSource::Type | PathSource::Expr(..) | PathSource::Pat |
            PathSource::Struct | PathSource::TupleStruct => true,
            PathSource::Trait | PathSource::TraitItem(..) |
            PathSource::Visibility | PathSource::ImportPrefix => false,
        }
    }

    fn descr_expected(self) -> &'static str {
        match self {
            PathSource::Type => "type",
            PathSource::Trait => "trait",
            PathSource::Pat => "unit struct/variant or constant",
            PathSource::Struct => "struct, variant or union type",
            PathSource::TupleStruct => "tuple struct/variant",
            PathSource::Visibility => "module",
            PathSource::ImportPrefix => "module or enum",
            PathSource::TraitItem(ns) => match ns {
                TypeNS => "associated type",
                ValueNS => "method or associated constant",
                MacroNS => bug!("associated macro"),
            },
            PathSource::Expr(parent) => match parent.map(|p| &p.node) {
                // "function" here means "anything callable" rather than `Def::Fn`,
                // this is not precise but usually more helpful than just "value".
                Some(&ExprKind::Call(..)) => "function",
                _ => "value",
            },
        }
    }

    fn is_expected(self, def: Def) -> bool {
        match self {
            PathSource::Type => match def {
                Def::Struct(..) | Def::Union(..) | Def::Enum(..) |
                Def::Trait(..) | Def::TyAlias(..) | Def::AssociatedTy(..) |
                Def::PrimTy(..) | Def::TyParam(..) | Def::SelfTy(..) => true,
                _ => false,
            },
            PathSource::Trait => match def {
                Def::Trait(..) => true,
                _ => false,
            },
            PathSource::Expr(..) => match def {
                Def::StructCtor(_, CtorKind::Const) | Def::StructCtor(_, CtorKind::Fn) |
                Def::VariantCtor(_, CtorKind::Const) | Def::VariantCtor(_, CtorKind::Fn) |
                Def::Const(..) | Def::Static(..) | Def::Local(..) | Def::Upvar(..) |
                Def::Fn(..) | Def::Method(..) | Def::AssociatedConst(..) => true,
                _ => false,
            },
            PathSource::Pat => match def {
                Def::StructCtor(_, CtorKind::Const) |
                Def::VariantCtor(_, CtorKind::Const) |
                Def::Const(..) | Def::AssociatedConst(..) => true,
                _ => false,
            },
            PathSource::TupleStruct => match def {
                Def::StructCtor(_, CtorKind::Fn) | Def::VariantCtor(_, CtorKind::Fn) => true,
                _ => false,
            },
            PathSource::Struct => match def {
                Def::Struct(..) | Def::Union(..) | Def::Variant(..) |
                Def::TyAlias(..) | Def::AssociatedTy(..) | Def::SelfTy(..) => true,
                _ => false,
            },
            PathSource::TraitItem(ns) => match def {
                Def::AssociatedConst(..) | Def::Method(..) if ns == ValueNS => true,
                Def::AssociatedTy(..) if ns == TypeNS => true,
                _ => false,
            },
            PathSource::ImportPrefix => match def {
                Def::Mod(..) | Def::Enum(..) => true,
                _ => false,
            },
            PathSource::Visibility => match def {
                Def::Mod(..) => true,
                _ => false,
            },
        }
    }

    fn error_code(self, has_unexpected_resolution: bool) -> &'static str {
        __diagnostic_used!(E0404);
        __diagnostic_used!(E0405);
        __diagnostic_used!(E0412);
        __diagnostic_used!(E0422);
        __diagnostic_used!(E0423);
        __diagnostic_used!(E0425);
        __diagnostic_used!(E0531);
        __diagnostic_used!(E0532);
        __diagnostic_used!(E0573);
        __diagnostic_used!(E0574);
        __diagnostic_used!(E0575);
        __diagnostic_used!(E0576);
        __diagnostic_used!(E0577);
        __diagnostic_used!(E0578);
        match (self, has_unexpected_resolution) {
            (PathSource::Trait, true) => "E0404",
            (PathSource::Trait, false) => "E0405",
            (PathSource::Type, true) => "E0573",
            (PathSource::Type, false) => "E0412",
            (PathSource::Struct, true) => "E0574",
            (PathSource::Struct, false) => "E0422",
            (PathSource::Expr(..), true) => "E0423",
            (PathSource::Expr(..), false) => "E0425",
            (PathSource::Pat, true) | (PathSource::TupleStruct, true) => "E0532",
            (PathSource::Pat, false) | (PathSource::TupleStruct, false) => "E0531",
            (PathSource::TraitItem(..), true) => "E0575",
            (PathSource::TraitItem(..), false) => "E0576",
            (PathSource::Visibility, true) | (PathSource::ImportPrefix, true) => "E0577",
            (PathSource::Visibility, false) | (PathSource::ImportPrefix, false) => "E0578",
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Namespace {
    TypeNS,
    ValueNS,
    MacroNS,
}

#[derive(Clone, Default, Debug)]
pub struct PerNS<T> {
    value_ns: T,
    type_ns: T,
    macro_ns: Option<T>,
}

impl<T> ::std::ops::Index<Namespace> for PerNS<T> {
    type Output = T;
    fn index(&self, ns: Namespace) -> &T {
        match ns {
            ValueNS => &self.value_ns,
            TypeNS => &self.type_ns,
            MacroNS => self.macro_ns.as_ref().unwrap(),
        }
    }
}

impl<T> ::std::ops::IndexMut<Namespace> for PerNS<T> {
    fn index_mut(&mut self, ns: Namespace) -> &mut T {
        match ns {
            ValueNS => &mut self.value_ns,
            TypeNS => &mut self.type_ns,
            MacroNS => self.macro_ns.as_mut().unwrap(),
        }
    }
}

impl<'a, 'tcx> Visitor<'tcx> for Resolver<'a> {
    fn visit_item(&mut self, item: &'tcx Item) {
        self.resolve_item(item);
    }
    fn visit_arm(&mut self, arm: &'tcx Arm) {
        self.resolve_arm(arm);
    }
    fn visit_block(&mut self, block: &'tcx Block) {
        self.resolve_block(block);
    }
    fn visit_expr(&mut self, expr: &'tcx Expr) {
        self.resolve_expr(expr, None);
    }
    fn visit_local(&mut self, local: &'tcx Local) {
        self.resolve_local(local);
    }
    fn visit_ty(&mut self, ty: &'tcx Ty) {
        match ty.node {
            TyKind::Path(ref qself, ref path) => {
                self.smart_resolve_path(ty.id, qself.as_ref(), path, PathSource::Type);
            }
            TyKind::ImplicitSelf => {
                let self_ty = keywords::SelfType.ident();
                let def = self.resolve_ident_in_lexical_scope(self_ty, TypeNS, true, ty.span)
                              .map_or(Def::Err, |d| d.def());
                self.record_def(ty.id, PathResolution::new(def));
            }
            TyKind::Array(ref element, ref length) => {
                self.visit_ty(element);
                self.with_constant_rib(|this| {
                    this.visit_expr(length);
                });
                return;
            }
            _ => (),
        }
        visit::walk_ty(self, ty);
    }
    fn visit_poly_trait_ref(&mut self,
                            tref: &'tcx ast::PolyTraitRef,
                            m: &'tcx ast::TraitBoundModifier) {
        self.smart_resolve_path(tref.trait_ref.ref_id, None,
                                &tref.trait_ref.path, PathSource::Trait);
        visit::walk_poly_trait_ref(self, tref, m);
    }
    fn visit_variant(&mut self,
                     variant: &'tcx ast::Variant,
                     generics: &'tcx Generics,
                     item_id: ast::NodeId) {
        if let Some(ref dis_expr) = variant.node.disr_expr {
            // resolve the discriminator expr as a constant
            self.with_constant_rib(|this| {
                this.visit_expr(dis_expr);
            });
        }

        // `visit::walk_variant` without the discriminant expression.
        self.visit_variant_data(&variant.node.data,
                                variant.node.name,
                                generics,
                                item_id,
                                variant.span);
    }
    fn visit_foreign_item(&mut self, foreign_item: &'tcx ForeignItem) {
        let type_parameters = match foreign_item.node {
            ForeignItemKind::Fn(_, ref generics) => {
                HasTypeParameters(generics, ItemRibKind)
            }
            ForeignItemKind::Static(..) => NoTypeParameters,
        };
        self.with_type_parameter_rib(type_parameters, |this| {
            visit::walk_foreign_item(this, foreign_item);
        });
    }
    fn visit_fn(&mut self,
                function_kind: FnKind<'tcx>,
                declaration: &'tcx FnDecl,
                _: Span,
                node_id: NodeId) {
        let rib_kind = match function_kind {
            FnKind::ItemFn(_, generics, ..) => {
                self.visit_generics(generics);
                ItemRibKind
            }
            FnKind::Method(_, sig, _, _) => {
                self.visit_generics(&sig.generics);
                MethodRibKind(!sig.decl.has_self())
            }
            FnKind::Closure(_) => ClosureRibKind(node_id),
        };

        // Create a value rib for the function.
        self.ribs[ValueNS].push(Rib::new(rib_kind));

        // Create a label rib for the function.
        self.label_ribs.push(Rib::new(rib_kind));

        // Add each argument to the rib.
        let mut bindings_list = FxHashMap();
        for argument in &declaration.inputs {
            self.resolve_pattern(&argument.pat, PatternSource::FnParam, &mut bindings_list);

            self.visit_ty(&argument.ty);

            debug!("(resolving function) recorded argument");
        }
        visit::walk_fn_ret_ty(self, &declaration.output);

        // Resolve the function body.
        match function_kind {
            FnKind::ItemFn(.., body) |
            FnKind::Method(.., body) => {
                self.visit_block(body);
            }
            FnKind::Closure(body) => {
                self.visit_expr(body);
            }
        };

        debug!("(resolving function) leaving function");

        self.label_ribs.pop();
        self.ribs[ValueNS].pop();
    }
    fn visit_generics(&mut self, generics: &'tcx Generics) {
        // For type parameter defaults, we have to ban access
        // to following type parameters, as the Substs can only
        // provide previous type parameters as they're built.
        let mut default_ban_rib = Rib::new(ForwardTyParamBanRibKind);
        default_ban_rib.bindings.extend(generics.ty_params.iter()
            .skip_while(|p| p.default.is_none())
            .map(|p| (Ident::with_empty_ctxt(p.ident.name), Def::Err)));

        for param in &generics.ty_params {
            for bound in &param.bounds {
                self.visit_ty_param_bound(bound);
            }

            if let Some(ref ty) = param.default {
                self.ribs[TypeNS].push(default_ban_rib);
                self.visit_ty(ty);
                default_ban_rib = self.ribs[TypeNS].pop().unwrap();
            }

            // Allow all following defaults to refer to this type parameter.
            default_ban_rib.bindings.remove(&Ident::with_empty_ctxt(param.ident.name));
        }
        for lt in &generics.lifetimes { self.visit_lifetime_def(lt); }
        for p in &generics.where_clause.predicates { self.visit_where_predicate(p); }
    }
}

pub type ErrorMessage = Option<(Span, String)>;

#[derive(Copy, Clone)]
enum TypeParameters<'a, 'b> {
    NoTypeParameters,
    HasTypeParameters(// Type parameters.
                      &'b Generics,

                      // The kind of the rib used for type parameters.
                      RibKind<'a>),
}

// The rib kind controls the translation of local
// definitions (`Def::Local`) to upvars (`Def::Upvar`).
#[derive(Copy, Clone, Debug)]
enum RibKind<'a> {
    // No translation needs to be applied.
    NormalRibKind,

    // We passed through a closure scope at the given node ID.
    // Translate upvars as appropriate.
    ClosureRibKind(NodeId /* func id */),

    // We passed through an impl or trait and are now in one of its
    // methods. Allow references to ty params that impl or trait
    // binds. Disallow any other upvars (including other ty params that are
    // upvars).
    //
    // The boolean value represents the fact that this method is static or not.
    MethodRibKind(bool),

    // We passed through an item scope. Disallow upvars.
    ItemRibKind,

    // We're in a constant item. Can't refer to dynamic stuff.
    ConstantItemRibKind,

    // We passed through a module.
    ModuleRibKind(Module<'a>),

    // We passed through a `macro_rules!` statement
    MacroDefinition(DefId),

    // All bindings in this rib are type parameters that can't be used
    // from the default of a type parameter because they're not declared
    // before said type parameter. Also see the `visit_generics` override.
    ForwardTyParamBanRibKind,
}

/// One local scope.
#[derive(Debug)]
struct Rib<'a> {
    bindings: FxHashMap<Ident, Def>,
    kind: RibKind<'a>,
}

impl<'a> Rib<'a> {
    fn new(kind: RibKind<'a>) -> Rib<'a> {
        Rib {
            bindings: FxHashMap(),
            kind: kind,
        }
    }
}

enum LexicalScopeBinding<'a> {
    Item(&'a NameBinding<'a>),
    Def(Def),
}

impl<'a> LexicalScopeBinding<'a> {
    fn item(self) -> Option<&'a NameBinding<'a>> {
        match self {
            LexicalScopeBinding::Item(binding) => Some(binding),
            _ => None,
        }
    }

    fn def(self) -> Def {
        match self {
            LexicalScopeBinding::Item(binding) => binding.def(),
            LexicalScopeBinding::Def(def) => def,
        }
    }
}

#[derive(Clone)]
enum PathResult<'a> {
    Module(Module<'a>),
    NonModule(PathResolution),
    Indeterminate,
    Failed(Span, String, bool /* is the error from the last segment? */),
}

enum ModuleKind {
    Block(NodeId),
    Def(Def, Name),
}

/// One node in the tree of modules.
pub struct ModuleData<'a> {
    parent: Option<Module<'a>>,
    kind: ModuleKind,

    // The def id of the closest normal module (`mod`) ancestor (including this module).
    normal_ancestor_id: DefId,

    resolutions: RefCell<FxHashMap<(Ident, Namespace), &'a RefCell<NameResolution<'a>>>>,
    legacy_macro_resolutions: RefCell<Vec<(Mark, Ident, Span, MacroKind)>>,
    macro_resolutions: RefCell<Vec<(Box<[Ident]>, Span)>>,

    // Macro invocations that can expand into items in this module.
    unresolved_invocations: RefCell<FxHashSet<Mark>>,

    no_implicit_prelude: bool,

    glob_importers: RefCell<Vec<&'a ImportDirective<'a>>>,
    globs: RefCell<Vec<&'a ImportDirective<'a>>>,

    // Used to memoize the traits in this module for faster searches through all traits in scope.
    traits: RefCell<Option<Box<[(Ident, &'a NameBinding<'a>)]>>>,

    // Whether this module is populated. If not populated, any attempt to
    // access the children must be preceded with a
    // `populate_module_if_necessary` call.
    populated: Cell<bool>,

    /// Span of the module itself. Used for error reporting.
    span: Span,

    expansion: Mark,
}

pub type Module<'a> = &'a ModuleData<'a>;

impl<'a> ModuleData<'a> {
    fn new(parent: Option<Module<'a>>,
           kind: ModuleKind,
           normal_ancestor_id: DefId,
           expansion: Mark,
           span: Span) -> Self {
        ModuleData {
            parent: parent,
            kind: kind,
            normal_ancestor_id: normal_ancestor_id,
            resolutions: RefCell::new(FxHashMap()),
            legacy_macro_resolutions: RefCell::new(Vec::new()),
            macro_resolutions: RefCell::new(Vec::new()),
            unresolved_invocations: RefCell::new(FxHashSet()),
            no_implicit_prelude: false,
            glob_importers: RefCell::new(Vec::new()),
            globs: RefCell::new((Vec::new())),
            traits: RefCell::new(None),
            populated: Cell::new(normal_ancestor_id.is_local()),
            span: span,
            expansion: expansion,
        }
    }

    fn for_each_child<F: FnMut(Ident, Namespace, &'a NameBinding<'a>)>(&self, mut f: F) {
        for (&(ident, ns), name_resolution) in self.resolutions.borrow().iter() {
            name_resolution.borrow().binding.map(|binding| f(ident, ns, binding));
        }
    }

    fn for_each_child_stable<F: FnMut(Ident, Namespace, &'a NameBinding<'a>)>(&self, mut f: F) {
        let resolutions = self.resolutions.borrow();
        let mut resolutions = resolutions.iter().map(|(&(ident, ns), &resolution)| {
                                                    // Pre-compute keys for sorting
                                                    (ident.name.as_str(), ns, ident, resolution)
                                                })
                                                .collect::<Vec<_>>();
        resolutions.sort_unstable_by_key(|&(str, ns, ..)| (str, ns));
        for &(_, ns, ident, resolution) in resolutions.iter() {
            resolution.borrow().binding.map(|binding| f(ident, ns, binding));
        }
    }

    fn def(&self) -> Option<Def> {
        match self.kind {
            ModuleKind::Def(def, _) => Some(def),
            _ => None,
        }
    }

    fn def_id(&self) -> Option<DefId> {
        self.def().as_ref().map(Def::def_id)
    }

    // `self` resolves to the first module ancestor that `is_normal`.
    fn is_normal(&self) -> bool {
        match self.kind {
            ModuleKind::Def(Def::Mod(_), _) => true,
            _ => false,
        }
    }

    fn is_trait(&self) -> bool {
        match self.kind {
            ModuleKind::Def(Def::Trait(_), _) => true,
            _ => false,
        }
    }

    fn is_local(&self) -> bool {
        self.normal_ancestor_id.is_local()
    }

    fn nearest_item_scope(&'a self) -> Module<'a> {
        if self.is_trait() { self.parent.unwrap() } else { self }
    }
}

impl<'a> fmt::Debug for ModuleData<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.def())
    }
}

// Records a possibly-private value, type, or module definition.
#[derive(Clone, Debug)]
pub struct NameBinding<'a> {
    kind: NameBindingKind<'a>,
    expansion: Mark,
    span: Span,
    vis: ty::Visibility,
}

pub trait ToNameBinding<'a> {
    fn to_name_binding(self, arenas: &'a ResolverArenas<'a>) -> &'a NameBinding<'a>;
}

impl<'a> ToNameBinding<'a> for &'a NameBinding<'a> {
    fn to_name_binding(self, _: &'a ResolverArenas<'a>) -> &'a NameBinding<'a> {
        self
    }
}

#[derive(Clone, Debug)]
enum NameBindingKind<'a> {
    Def(Def),
    Module(Module<'a>),
    Import {
        binding: &'a NameBinding<'a>,
        directive: &'a ImportDirective<'a>,
        used: Cell<bool>,
        legacy_self_import: bool,
    },
    Ambiguity {
        b1: &'a NameBinding<'a>,
        b2: &'a NameBinding<'a>,
        legacy: bool,
    }
}

struct PrivacyError<'a>(Span, Name, &'a NameBinding<'a>);

struct AmbiguityError<'a> {
    span: Span,
    name: Name,
    lexical: bool,
    b1: &'a NameBinding<'a>,
    b2: &'a NameBinding<'a>,
    legacy: bool,
}

impl<'a> NameBinding<'a> {
    fn module(&self) -> Option<Module<'a>> {
        match self.kind {
            NameBindingKind::Module(module) => Some(module),
            NameBindingKind::Import { binding, .. } => binding.module(),
            NameBindingKind::Ambiguity { legacy: true, b1, .. } => b1.module(),
            _ => None,
        }
    }

    fn def(&self) -> Def {
        match self.kind {
            NameBindingKind::Def(def) => def,
            NameBindingKind::Module(module) => module.def().unwrap(),
            NameBindingKind::Import { binding, .. } => binding.def(),
            NameBindingKind::Ambiguity { legacy: true, b1, .. } => b1.def(),
            NameBindingKind::Ambiguity { .. } => Def::Err,
        }
    }

    fn def_ignoring_ambiguity(&self) -> Def {
        match self.kind {
            NameBindingKind::Import { binding, .. } => binding.def_ignoring_ambiguity(),
            NameBindingKind::Ambiguity { b1, .. } => b1.def_ignoring_ambiguity(),
            _ => self.def(),
        }
    }

    fn get_macro(&self, resolver: &mut Resolver<'a>) -> Rc<SyntaxExtension> {
        resolver.get_macro(self.def_ignoring_ambiguity())
    }

    // We sometimes need to treat variants as `pub` for backwards compatibility
    fn pseudo_vis(&self) -> ty::Visibility {
        if self.is_variant() { ty::Visibility::Public } else { self.vis }
    }

    fn is_variant(&self) -> bool {
        match self.kind {
            NameBindingKind::Def(Def::Variant(..)) |
            NameBindingKind::Def(Def::VariantCtor(..)) => true,
            _ => false,
        }
    }

    fn is_extern_crate(&self) -> bool {
        match self.kind {
            NameBindingKind::Import {
                directive: &ImportDirective {
                    subclass: ImportDirectiveSubclass::ExternCrate, ..
                }, ..
            } => true,
            _ => false,
        }
    }

    fn is_import(&self) -> bool {
        match self.kind {
            NameBindingKind::Import { .. } => true,
            _ => false,
        }
    }

    fn is_glob_import(&self) -> bool {
        match self.kind {
            NameBindingKind::Import { directive, .. } => directive.is_glob(),
            NameBindingKind::Ambiguity { b1, .. } => b1.is_glob_import(),
            _ => false,
        }
    }

    fn is_importable(&self) -> bool {
        match self.def() {
            Def::AssociatedConst(..) | Def::Method(..) | Def::AssociatedTy(..) => false,
            _ => true,
        }
    }

    fn is_macro_def(&self) -> bool {
        match self.kind {
            NameBindingKind::Def(Def::Macro(..)) => true,
            _ => false,
        }
    }

    fn descr(&self) -> &'static str {
        if self.is_extern_crate() { "extern crate" } else { self.def().kind_name() }
    }
}

/// Interns the names of the primitive types.
struct PrimitiveTypeTable {
    primitive_types: FxHashMap<Name, PrimTy>,
}

impl PrimitiveTypeTable {
    fn new() -> PrimitiveTypeTable {
        let mut table = PrimitiveTypeTable { primitive_types: FxHashMap() };

        table.intern("bool", TyBool);
        table.intern("char", TyChar);
        table.intern("f32", TyFloat(FloatTy::F32));
        table.intern("f64", TyFloat(FloatTy::F64));
        table.intern("isize", TyInt(IntTy::Is));
        table.intern("i8", TyInt(IntTy::I8));
        table.intern("i16", TyInt(IntTy::I16));
        table.intern("i32", TyInt(IntTy::I32));
        table.intern("i64", TyInt(IntTy::I64));
        table.intern("i128", TyInt(IntTy::I128));
        table.intern("str", TyStr);
        table.intern("usize", TyUint(UintTy::Us));
        table.intern("u8", TyUint(UintTy::U8));
        table.intern("u16", TyUint(UintTy::U16));
        table.intern("u32", TyUint(UintTy::U32));
        table.intern("u64", TyUint(UintTy::U64));
        table.intern("u128", TyUint(UintTy::U128));
        table
    }

    fn intern(&mut self, string: &str, primitive_type: PrimTy) {
        self.primitive_types.insert(Symbol::intern(string), primitive_type);
    }
}

/// The main resolver class.
pub struct Resolver<'a> {
    session: &'a Session,

    pub definitions: Definitions,

    graph_root: Module<'a>,

    prelude: Option<Module<'a>>,

    // n.b. This is used only for better diagnostics, not name resolution itself.
    has_self: FxHashSet<DefId>,

    // Names of fields of an item `DefId` accessible with dot syntax.
    // Used for hints during error reporting.
    field_names: FxHashMap<DefId, Vec<Name>>,

    // All imports known to succeed or fail.
    determined_imports: Vec<&'a ImportDirective<'a>>,

    // All non-determined imports.
    indeterminate_imports: Vec<&'a ImportDirective<'a>>,

    // The module that represents the current item scope.
    current_module: Module<'a>,

    // The current set of local scopes for types and values.
    // FIXME #4948: Reuse ribs to avoid allocation.
    ribs: PerNS<Vec<Rib<'a>>>,

    // The current set of local scopes, for labels.
    label_ribs: Vec<Rib<'a>>,

    // The trait that the current context can refer to.
    current_trait_ref: Option<(Module<'a>, TraitRef)>,

    // The current self type if inside an impl (used for better errors).
    current_self_type: Option<Ty>,

    // The idents for the primitive types.
    primitive_type_table: PrimitiveTypeTable,

    def_map: DefMap,
    pub freevars: FreevarMap,
    freevars_seen: NodeMap<NodeMap<usize>>,
    pub export_map: ExportMap,
    pub trait_map: TraitMap,

    // A map from nodes to anonymous modules.
    // Anonymous modules are pseudo-modules that are implicitly created around items
    // contained within blocks.
    //
    // For example, if we have this:
    //
    //  fn f() {
    //      fn g() {
    //          ...
    //      }
    //  }
    //
    // There will be an anonymous module created around `g` with the ID of the
    // entry block for `f`.
    block_map: NodeMap<Module<'a>>,
    module_map: FxHashMap<DefId, Module<'a>>,
    extern_module_map: FxHashMap<(DefId, bool /* MacrosOnly? */), Module<'a>>,

    pub make_glob_map: bool,
    // Maps imports to the names of items actually imported (this actually maps
    // all imports, but only glob imports are actually interesting).
    pub glob_map: GlobMap,

    used_imports: FxHashSet<(NodeId, Namespace)>,
    pub maybe_unused_trait_imports: NodeSet,

    privacy_errors: Vec<PrivacyError<'a>>,
    ambiguity_errors: Vec<AmbiguityError<'a>>,
    gated_errors: FxHashSet<Span>,
    disallowed_shadowing: Vec<&'a LegacyBinding<'a>>,

    arenas: &'a ResolverArenas<'a>,
    dummy_binding: &'a NameBinding<'a>,
    use_extern_macros: bool, // true if `#![feature(use_extern_macros)]`

    crate_loader: &'a mut CrateLoader,
    macro_names: FxHashSet<Ident>,
    global_macros: FxHashMap<Name, &'a NameBinding<'a>>,
    lexical_macro_resolutions: Vec<(Ident, &'a Cell<LegacyScope<'a>>)>,
    macro_map: FxHashMap<DefId, Rc<SyntaxExtension>>,
    macro_defs: FxHashMap<Mark, DefId>,
    local_macro_def_scopes: FxHashMap<NodeId, Module<'a>>,
    macro_exports: Vec<Export>,
    pub whitelisted_legacy_custom_derives: Vec<Name>,
    pub found_unresolved_macro: bool,

    // List of crate local macros that we need to warn about as being unused.
    // Right now this only includes macro_rules! macros, and macros 2.0.
    unused_macros: FxHashSet<DefId>,

    // Maps the `Mark` of an expansion to its containing module or block.
    invocations: FxHashMap<Mark, &'a InvocationData<'a>>,

    // Avoid duplicated errors for "name already defined".
    name_already_seen: FxHashMap<Name, Span>,

    // If `#![feature(proc_macro)]` is set
    proc_macro_enabled: bool,

    // A set of procedural macros imported by `#[macro_use]` that have already been warned about
    warned_proc_macros: FxHashSet<Name>,

    potentially_unused_imports: Vec<&'a ImportDirective<'a>>,

    // This table maps struct IDs into struct constructor IDs,
    // it's not used during normal resolution, only for better error reporting.
    struct_constructors: DefIdMap<(Def, ty::Visibility)>,

    // Only used for better errors on `fn(): fn()`
    current_type_ascription: Vec<Span>,
}

pub struct ResolverArenas<'a> {
    modules: arena::TypedArena<ModuleData<'a>>,
    local_modules: RefCell<Vec<Module<'a>>>,
    name_bindings: arena::TypedArena<NameBinding<'a>>,
    import_directives: arena::TypedArena<ImportDirective<'a>>,
    name_resolutions: arena::TypedArena<RefCell<NameResolution<'a>>>,
    invocation_data: arena::TypedArena<InvocationData<'a>>,
    legacy_bindings: arena::TypedArena<LegacyBinding<'a>>,
}

impl<'a> ResolverArenas<'a> {
    fn alloc_module(&'a self, module: ModuleData<'a>) -> Module<'a> {
        let module = self.modules.alloc(module);
        if module.def_id().map(|def_id| def_id.is_local()).unwrap_or(true) {
            self.local_modules.borrow_mut().push(module);
        }
        module
    }
    fn local_modules(&'a self) -> ::std::cell::Ref<'a, Vec<Module<'a>>> {
        self.local_modules.borrow()
    }
    fn alloc_name_binding(&'a self, name_binding: NameBinding<'a>) -> &'a NameBinding<'a> {
        self.name_bindings.alloc(name_binding)
    }
    fn alloc_import_directive(&'a self, import_directive: ImportDirective<'a>)
                              -> &'a ImportDirective {
        self.import_directives.alloc(import_directive)
    }
    fn alloc_name_resolution(&'a self) -> &'a RefCell<NameResolution<'a>> {
        self.name_resolutions.alloc(Default::default())
    }
    fn alloc_invocation_data(&'a self, expansion_data: InvocationData<'a>)
                             -> &'a InvocationData<'a> {
        self.invocation_data.alloc(expansion_data)
    }
    fn alloc_legacy_binding(&'a self, binding: LegacyBinding<'a>) -> &'a LegacyBinding<'a> {
        self.legacy_bindings.alloc(binding)
    }
}

impl<'a, 'b: 'a> ty::DefIdTree for &'a Resolver<'b> {
    fn parent(self, id: DefId) -> Option<DefId> {
        match id.krate {
            LOCAL_CRATE => self.definitions.def_key(id.index).parent,
            _ => self.session.cstore.def_key(id).parent,
        }.map(|index| DefId { index: index, ..id })
    }
}

impl<'a> hir::lowering::Resolver for Resolver<'a> {
    fn resolve_hir_path(&mut self, path: &mut hir::Path, is_value: bool) {
        let namespace = if is_value { ValueNS } else { TypeNS };
        let hir::Path { ref segments, span, ref mut def } = *path;
        let path: Vec<SpannedIdent> = segments.iter()
            .map(|seg| respan(span, Ident::with_empty_ctxt(seg.name)))
            .collect();
        match self.resolve_path(&path, Some(namespace), true, span) {
            PathResult::Module(module) => *def = module.def().unwrap(),
            PathResult::NonModule(path_res) if path_res.unresolved_segments() == 0 =>
                *def = path_res.base_def(),
            PathResult::NonModule(..) => match self.resolve_path(&path, None, true, span) {
                PathResult::Failed(span, msg, _) => {
                    resolve_error(self, span, ResolutionError::FailedToResolve(&msg));
                }
                _ => {}
            },
            PathResult::Indeterminate => unreachable!(),
            PathResult::Failed(span, msg, _) => {
                resolve_error(self, span, ResolutionError::FailedToResolve(&msg));
            }
        }
    }

    fn get_resolution(&mut self, id: NodeId) -> Option<PathResolution> {
        self.def_map.get(&id).cloned()
    }

    fn definitions(&mut self) -> &mut Definitions {
        &mut self.definitions
    }
}

impl<'a> Resolver<'a> {
    pub fn new(session: &'a Session,
               krate: &Crate,
               crate_name: &str,
               make_glob_map: MakeGlobMap,
               crate_loader: &'a mut CrateLoader,
               arenas: &'a ResolverArenas<'a>)
               -> Resolver<'a> {
        let root_def_id = DefId::local(CRATE_DEF_INDEX);
        let root_module_kind = ModuleKind::Def(Def::Mod(root_def_id), keywords::Invalid.name());
        let graph_root = arenas.alloc_module(ModuleData {
            no_implicit_prelude: attr::contains_name(&krate.attrs, "no_implicit_prelude"),
            ..ModuleData::new(None, root_module_kind, root_def_id, Mark::root(), krate.span)
        });
        let mut module_map = FxHashMap();
        module_map.insert(DefId::local(CRATE_DEF_INDEX), graph_root);

        let mut definitions = Definitions::new();
        DefCollector::new(&mut definitions, Mark::root())
            .collect_root(crate_name, &session.local_crate_disambiguator().as_str());

        let mut invocations = FxHashMap();
        invocations.insert(Mark::root(),
                           arenas.alloc_invocation_data(InvocationData::root(graph_root)));

        let features = session.features.borrow();

        let mut macro_defs = FxHashMap();
        macro_defs.insert(Mark::root(), root_def_id);

        Resolver {
            session: session,

            definitions: definitions,

            // The outermost module has def ID 0; this is not reflected in the
            // AST.
            graph_root: graph_root,
            prelude: None,

            has_self: FxHashSet(),
            field_names: FxHashMap(),

            determined_imports: Vec::new(),
            indeterminate_imports: Vec::new(),

            current_module: graph_root,
            ribs: PerNS {
                value_ns: vec![Rib::new(ModuleRibKind(graph_root))],
                type_ns: vec![Rib::new(ModuleRibKind(graph_root))],
                macro_ns: Some(vec![Rib::new(ModuleRibKind(graph_root))]),
            },
            label_ribs: Vec::new(),

            current_trait_ref: None,
            current_self_type: None,

            primitive_type_table: PrimitiveTypeTable::new(),

            def_map: NodeMap(),
            freevars: NodeMap(),
            freevars_seen: NodeMap(),
            export_map: NodeMap(),
            trait_map: NodeMap(),
            module_map: module_map,
            block_map: NodeMap(),
            extern_module_map: FxHashMap(),

            make_glob_map: make_glob_map == MakeGlobMap::Yes,
            glob_map: NodeMap(),

            used_imports: FxHashSet(),
            maybe_unused_trait_imports: NodeSet(),

            privacy_errors: Vec::new(),
            ambiguity_errors: Vec::new(),
            gated_errors: FxHashSet(),
            disallowed_shadowing: Vec::new(),

            arenas: arenas,
            dummy_binding: arenas.alloc_name_binding(NameBinding {
                kind: NameBindingKind::Def(Def::Err),
                expansion: Mark::root(),
                span: DUMMY_SP,
                vis: ty::Visibility::Public,
            }),

            // The `proc_macro` and `decl_macro` features imply `use_extern_macros`
            use_extern_macros:
                features.use_extern_macros || features.proc_macro || features.decl_macro,

            crate_loader: crate_loader,
            macro_names: FxHashSet(),
            global_macros: FxHashMap(),
            lexical_macro_resolutions: Vec::new(),
            macro_map: FxHashMap(),
            macro_exports: Vec::new(),
            invocations: invocations,
            macro_defs: macro_defs,
            local_macro_def_scopes: FxHashMap(),
            name_already_seen: FxHashMap(),
            whitelisted_legacy_custom_derives: Vec::new(),
            proc_macro_enabled: features.proc_macro,
            warned_proc_macros: FxHashSet(),
            potentially_unused_imports: Vec::new(),
            struct_constructors: DefIdMap(),
            found_unresolved_macro: false,
            unused_macros: FxHashSet(),
            current_type_ascription: Vec::new(),
        }
    }

    pub fn arenas() -> ResolverArenas<'a> {
        ResolverArenas {
            modules: arena::TypedArena::new(),
            local_modules: RefCell::new(Vec::new()),
            name_bindings: arena::TypedArena::new(),
            import_directives: arena::TypedArena::new(),
            name_resolutions: arena::TypedArena::new(),
            invocation_data: arena::TypedArena::new(),
            legacy_bindings: arena::TypedArena::new(),
        }
    }

    fn per_ns<T, F: FnMut(&mut Self, Namespace) -> T>(&mut self, mut f: F) -> PerNS<T> {
        PerNS {
            type_ns: f(self, TypeNS),
            value_ns: f(self, ValueNS),
            macro_ns: match self.use_extern_macros {
                true => Some(f(self, MacroNS)),
                false => None,
            },
        }
    }

    /// Entry point to crate resolution.
    pub fn resolve_crate(&mut self, krate: &Crate) {
        ImportResolver { resolver: self }.finalize_imports();
        self.current_module = self.graph_root;
        self.finalize_current_module_macro_resolutions();
        visit::walk_crate(self, krate);

        check_unused::check_crate(self, krate);
        self.report_errors();
        self.crate_loader.postprocess(krate);
    }

    fn new_module(
        &self,
        parent: Module<'a>,
        kind: ModuleKind,
        normal_ancestor_id: DefId,
        expansion: Mark,
        span: Span,
    ) -> Module<'a> {
        let module = ModuleData::new(Some(parent), kind, normal_ancestor_id, expansion, span);
        self.arenas.alloc_module(module)
    }

    fn record_use(&mut self, ident: Ident, ns: Namespace, binding: &'a NameBinding<'a>, span: Span)
                  -> bool /* true if an error was reported */ {
        match binding.kind {
            NameBindingKind::Import { directive, binding, ref used, legacy_self_import }
                    if !used.get() => {
                used.set(true);
                directive.used.set(true);
                if legacy_self_import {
                    self.warn_legacy_self_import(directive);
                    return false;
                }
                self.used_imports.insert((directive.id, ns));
                self.add_to_glob_map(directive.id, ident);
                self.record_use(ident, ns, binding, span)
            }
            NameBindingKind::Import { .. } => false,
            NameBindingKind::Ambiguity { b1, b2, legacy } => {
                self.ambiguity_errors.push(AmbiguityError {
                    span: span, name: ident.name, lexical: false, b1: b1, b2: b2, legacy: legacy,
                });
                if legacy {
                    self.record_use(ident, ns, b1, span);
                }
                !legacy
            }
            _ => false
        }
    }

    fn add_to_glob_map(&mut self, id: NodeId, ident: Ident) {
        if self.make_glob_map {
            self.glob_map.entry(id).or_insert_with(FxHashSet).insert(ident.name);
        }
    }

    /// This resolves the identifier `ident` in the namespace `ns` in the current lexical scope.
    /// More specifically, we proceed up the hierarchy of scopes and return the binding for
    /// `ident` in the first scope that defines it (or None if no scopes define it).
    ///
    /// A block's items are above its local variables in the scope hierarchy, regardless of where
    /// the items are defined in the block. For example,
    /// ```rust
    /// fn f() {
    ///    g(); // Since there are no local variables in scope yet, this resolves to the item.
    ///    let g = || {};
    ///    fn g() {}
    ///    g(); // This resolves to the local variable `g` since it shadows the item.
    /// }
    /// ```
    ///
    /// Invariant: This must only be called during main resolution, not during
    /// import resolution.
    fn resolve_ident_in_lexical_scope(&mut self,
                                      mut ident: Ident,
                                      ns: Namespace,
                                      record_used: bool,
                                      path_span: Span)
                                      -> Option<LexicalScopeBinding<'a>> {
        if ns == TypeNS {
            ident.ctxt = if ident.name == keywords::SelfType.name() {
                SyntaxContext::empty() // FIXME(jseyfried) improve `Self` hygiene
            } else {
                ident.ctxt.modern()
            }
        }

        // Walk backwards up the ribs in scope.
        let mut module = self.graph_root;
        for i in (0 .. self.ribs[ns].len()).rev() {
            if let Some(def) = self.ribs[ns][i].bindings.get(&ident).cloned() {
                // The ident resolves to a type parameter or local variable.
                return Some(LexicalScopeBinding::Def(
                    self.adjust_local_def(ns, i, def, record_used, path_span)
                ));
            }

            module = match self.ribs[ns][i].kind {
                ModuleRibKind(module) => module,
                MacroDefinition(def) if def == self.macro_defs[&ident.ctxt.outer()] => {
                    // If an invocation of this macro created `ident`, give up on `ident`
                    // and switch to `ident`'s source from the macro definition.
                    ident.ctxt.remove_mark();
                    continue
                }
                _ => continue,
            };

            let item = self.resolve_ident_in_module_unadjusted(
                module, ident, ns, false, record_used, path_span,
            );
            if let Ok(binding) = item {
                // The ident resolves to an item.
                return Some(LexicalScopeBinding::Item(binding));
            }

            match module.kind {
                ModuleKind::Block(..) => {}, // We can see through blocks
                _ => break,
            }
        }

        ident.ctxt = ident.ctxt.modern();
        loop {
            module = unwrap_or!(self.hygienic_lexical_parent(module, &mut ident.ctxt), break);
            let orig_current_module = self.current_module;
            self.current_module = module; // Lexical resolutions can never be a privacy error.
            let result = self.resolve_ident_in_module_unadjusted(
                module, ident, ns, false, record_used, path_span,
            );
            self.current_module = orig_current_module;

            match result {
                Ok(binding) => return Some(LexicalScopeBinding::Item(binding)),
                Err(Undetermined) => return None,
                Err(Determined) => {}
            }
        }

        match self.prelude {
            Some(prelude) if !module.no_implicit_prelude => {
                self.resolve_ident_in_module_unadjusted(prelude, ident, ns, false, false, path_span)
                    .ok().map(LexicalScopeBinding::Item)
            }
            _ => None,
        }
    }

    fn hygienic_lexical_parent(&mut self, mut module: Module<'a>, ctxt: &mut SyntaxContext)
                               -> Option<Module<'a>> {
        if !module.expansion.is_descendant_of(ctxt.outer()) {
            return Some(self.macro_def_scope(ctxt.remove_mark()));
        }

        if let ModuleKind::Block(..) = module.kind {
            return Some(module.parent.unwrap());
        }

        let mut module_expansion = module.expansion.modern(); // for backward compatability
        while let Some(parent) = module.parent {
            let parent_expansion = parent.expansion.modern();
            if module_expansion.is_descendant_of(parent_expansion) &&
               parent_expansion != module_expansion {
                return if parent_expansion.is_descendant_of(ctxt.outer()) {
                    Some(parent)
                } else {
                    None
                };
            }
            module = parent;
            module_expansion = parent_expansion;
        }

        None
    }

    fn resolve_ident_in_module(&mut self,
                               module: Module<'a>,
                               mut ident: Ident,
                               ns: Namespace,
                               ignore_unresolved_invocations: bool,
                               record_used: bool,
                               span: Span)
                               -> Result<&'a NameBinding<'a>, Determinacy> {
        ident.ctxt = ident.ctxt.modern();
        let orig_current_module = self.current_module;
        if let Some(def) = ident.ctxt.adjust(module.expansion) {
            self.current_module = self.macro_def_scope(def);
        }
        let result = self.resolve_ident_in_module_unadjusted(
            module, ident, ns, ignore_unresolved_invocations, record_used, span,
        );
        self.current_module = orig_current_module;
        result
    }

    fn resolve_crate_root(&mut self, mut ctxt: SyntaxContext) -> Module<'a> {
        let module = match ctxt.adjust(Mark::root()) {
            Some(def) => self.macro_def_scope(def),
            None => return self.graph_root,
        };
        self.get_module(DefId { index: CRATE_DEF_INDEX, ..module.normal_ancestor_id })
    }

    fn resolve_self(&mut self, ctxt: &mut SyntaxContext, module: Module<'a>) -> Module<'a> {
        let mut module = self.get_module(module.normal_ancestor_id);
        while module.span.ctxt.modern() != *ctxt {
            let parent = module.parent.unwrap_or_else(|| self.macro_def_scope(ctxt.remove_mark()));
            module = self.get_module(parent.normal_ancestor_id);
        }
        module
    }

    // AST resolution
    //
    // We maintain a list of value ribs and type ribs.
    //
    // Simultaneously, we keep track of the current position in the module
    // graph in the `current_module` pointer. When we go to resolve a name in
    // the value or type namespaces, we first look through all the ribs and
    // then query the module graph. When we resolve a name in the module
    // namespace, we can skip all the ribs (since nested modules are not
    // allowed within blocks in Rust) and jump straight to the current module
    // graph node.
    //
    // Named implementations are handled separately. When we find a method
    // call, we consult the module node to find all of the implementations in
    // scope. This information is lazily cached in the module node. We then
    // generate a fake "implementation scope" containing all the
    // implementations thus found, for compatibility with old resolve pass.

    fn with_scope<F>(&mut self, id: NodeId, f: F)
        where F: FnOnce(&mut Resolver)
    {
        let id = self.definitions.local_def_id(id);
        let module = self.module_map.get(&id).cloned(); // clones a reference
        if let Some(module) = module {
            // Move down in the graph.
            let orig_module = replace(&mut self.current_module, module);
            self.ribs[ValueNS].push(Rib::new(ModuleRibKind(module)));
            self.ribs[TypeNS].push(Rib::new(ModuleRibKind(module)));

            self.finalize_current_module_macro_resolutions();
            f(self);

            self.current_module = orig_module;
            self.ribs[ValueNS].pop();
            self.ribs[TypeNS].pop();
        } else {
            f(self);
        }
    }

    /// Searches the current set of local scopes for labels.
    /// Stops after meeting a closure.
    fn search_label(&self, mut ident: Ident) -> Option<Def> {
        for rib in self.label_ribs.iter().rev() {
            match rib.kind {
                NormalRibKind => {}
                // If an invocation of this macro created `ident`, give up on `ident`
                // and switch to `ident`'s source from the macro definition.
                MacroDefinition(def) => {
                    if def == self.macro_defs[&ident.ctxt.outer()] {
                        ident.ctxt.remove_mark();
                    }
                }
                _ => {
                    // Do not resolve labels across function boundary
                    return None;
                }
            }
            let result = rib.bindings.get(&ident).cloned();
            if result.is_some() {
                return result;
            }
        }
        None
    }

    fn resolve_item(&mut self, item: &Item) {
        let name = item.ident.name;

        debug!("(resolving item) resolving {}", name);

        self.check_proc_macro_attrs(&item.attrs);

        match item.node {
            ItemKind::Enum(_, ref generics) |
            ItemKind::Ty(_, ref generics) |
            ItemKind::Struct(_, ref generics) |
            ItemKind::Union(_, ref generics) |
            ItemKind::Fn(.., ref generics, _) => {
                self.with_type_parameter_rib(HasTypeParameters(generics, ItemRibKind),
                                             |this| visit::walk_item(this, item));
            }

            ItemKind::DefaultImpl(_, ref trait_ref) => {
                self.with_optional_trait_ref(Some(trait_ref), |this, _| {
                    // Resolve type arguments in trait path
                    visit::walk_trait_ref(this, trait_ref);
                });
            }
            ItemKind::Impl(.., ref generics, ref opt_trait_ref, ref self_type, ref impl_items) =>
                self.resolve_implementation(generics,
                                            opt_trait_ref,
                                            &self_type,
                                            item.id,
                                            impl_items),

            ItemKind::Trait(_, ref generics, ref bounds, ref trait_items) => {
                // Create a new rib for the trait-wide type parameters.
                self.with_type_parameter_rib(HasTypeParameters(generics, ItemRibKind), |this| {
                    let local_def_id = this.definitions.local_def_id(item.id);
                    this.with_self_rib(Def::SelfTy(Some(local_def_id), None), |this| {
                        this.visit_generics(generics);
                        walk_list!(this, visit_ty_param_bound, bounds);

                        for trait_item in trait_items {
                            this.check_proc_macro_attrs(&trait_item.attrs);

                            match trait_item.node {
                                TraitItemKind::Const(ref ty, ref default) => {
                                    this.visit_ty(ty);

                                    // Only impose the restrictions of
                                    // ConstRibKind for an actual constant
                                    // expression in a provided default.
                                    if let Some(ref expr) = *default{
                                        this.with_constant_rib(|this| {
                                            this.visit_expr(expr);
                                        });
                                    }
                                }
                                TraitItemKind::Method(ref sig, _) => {
                                    let type_parameters =
                                        HasTypeParameters(&sig.generics,
                                                          MethodRibKind(!sig.decl.has_self()));
                                    this.with_type_parameter_rib(type_parameters, |this| {
                                        visit::walk_trait_item(this, trait_item)
                                    });
                                }
                                TraitItemKind::Type(..) => {
                                    this.with_type_parameter_rib(NoTypeParameters, |this| {
                                        visit::walk_trait_item(this, trait_item)
                                    });
                                }
                                TraitItemKind::Macro(_) => panic!("unexpanded macro in resolve!"),
                            };
                        }
                    });
                });
            }

            ItemKind::Mod(_) | ItemKind::ForeignMod(_) => {
                self.with_scope(item.id, |this| {
                    visit::walk_item(this, item);
                });
            }

            ItemKind::Static(ref ty, _, ref expr) |
            ItemKind::Const(ref ty, ref expr) => {
                self.with_item_rib(|this| {
                    this.visit_ty(ty);
                    this.with_constant_rib(|this| {
                        this.visit_expr(expr);
                    });
                });
            }

            ItemKind::Use(ref view_path) => {
                match view_path.node {
                    ast::ViewPathList(ref prefix, ref items) if items.is_empty() => {
                        // Resolve prefix of an import with empty braces (issue #28388).
                        self.smart_resolve_path(item.id, None, prefix, PathSource::ImportPrefix);
                    }
                    _ => {}
                }
            }

            ItemKind::ExternCrate(_) | ItemKind::MacroDef(..) | ItemKind::GlobalAsm(_)=> {
                // do nothing, these are just around to be encoded
            }

            ItemKind::Mac(_) => panic!("unexpanded macro in resolve!"),
        }
    }

    fn with_type_parameter_rib<'b, F>(&'b mut self, type_parameters: TypeParameters<'a, 'b>, f: F)
        where F: FnOnce(&mut Resolver)
    {
        match type_parameters {
            HasTypeParameters(generics, rib_kind) => {
                let mut function_type_rib = Rib::new(rib_kind);
                let mut seen_bindings = FxHashMap();
                for type_parameter in &generics.ty_params {
                    let ident = type_parameter.ident.modern();
                    debug!("with_type_parameter_rib: {}", type_parameter.id);

                    if seen_bindings.contains_key(&ident) {
                        let span = seen_bindings.get(&ident).unwrap();
                        let err =
                            ResolutionError::NameAlreadyUsedInTypeParameterList(ident.name, span);
                        resolve_error(self, type_parameter.span, err);
                    }
                    seen_bindings.entry(ident).or_insert(type_parameter.span);

                    // plain insert (no renaming)
                    let def_id = self.definitions.local_def_id(type_parameter.id);
                    let def = Def::TyParam(def_id);
                    function_type_rib.bindings.insert(ident, def);
                    self.record_def(type_parameter.id, PathResolution::new(def));
                }
                self.ribs[TypeNS].push(function_type_rib);
            }

            NoTypeParameters => {
                // Nothing to do.
            }
        }

        f(self);

        if let HasTypeParameters(..) = type_parameters {
            self.ribs[TypeNS].pop();
        }
    }

    fn with_label_rib<F>(&mut self, f: F)
        where F: FnOnce(&mut Resolver)
    {
        self.label_ribs.push(Rib::new(NormalRibKind));
        f(self);
        self.label_ribs.pop();
    }

    fn with_item_rib<F>(&mut self, f: F)
        where F: FnOnce(&mut Resolver)
    {
        self.ribs[ValueNS].push(Rib::new(ItemRibKind));
        self.ribs[TypeNS].push(Rib::new(ItemRibKind));
        f(self);
        self.ribs[TypeNS].pop();
        self.ribs[ValueNS].pop();
    }

    fn with_constant_rib<F>(&mut self, f: F)
        where F: FnOnce(&mut Resolver)
    {
        self.ribs[ValueNS].push(Rib::new(ConstantItemRibKind));
        f(self);
        self.ribs[ValueNS].pop();
    }

    fn with_current_self_type<T, F>(&mut self, self_type: &Ty, f: F) -> T
        where F: FnOnce(&mut Resolver) -> T
    {
        // Handle nested impls (inside fn bodies)
        let previous_value = replace(&mut self.current_self_type, Some(self_type.clone()));
        let result = f(self);
        self.current_self_type = previous_value;
        result
    }

    fn with_optional_trait_ref<T, F>(&mut self, opt_trait_ref: Option<&TraitRef>, f: F) -> T
        where F: FnOnce(&mut Resolver, Option<DefId>) -> T
    {
        let mut new_val = None;
        let mut new_id = None;
        if let Some(trait_ref) = opt_trait_ref {
            let path: Vec<_> = trait_ref.path.segments.iter()
                .map(|seg| respan(seg.span, seg.identifier))
                .collect();
            let def = self.smart_resolve_path_fragment(trait_ref.ref_id,
                                                       None,
                                                       &path,
                                                       trait_ref.path.span,
                                                       trait_ref.path.segments.last().unwrap().span,
                                                       PathSource::Trait)
                .base_def();
            if def != Def::Err {
                new_id = Some(def.def_id());
                let span = trait_ref.path.span;
                if let PathResult::Module(module) = self.resolve_path(&path, None, false, span) {
                    new_val = Some((module, trait_ref.clone()));
                }
            }
        }
        let original_trait_ref = replace(&mut self.current_trait_ref, new_val);
        let result = f(self, new_id);
        self.current_trait_ref = original_trait_ref;
        result
    }

    fn with_self_rib<F>(&mut self, self_def: Def, f: F)
        where F: FnOnce(&mut Resolver)
    {
        let mut self_type_rib = Rib::new(NormalRibKind);

        // plain insert (no renaming, types are not currently hygienic....)
        self_type_rib.bindings.insert(keywords::SelfType.ident(), self_def);
        self.ribs[TypeNS].push(self_type_rib);
        f(self);
        self.ribs[TypeNS].pop();
    }

    fn resolve_implementation(&mut self,
                              generics: &Generics,
                              opt_trait_reference: &Option<TraitRef>,
                              self_type: &Ty,
                              item_id: NodeId,
                              impl_items: &[ImplItem]) {
        // If applicable, create a rib for the type parameters.
        self.with_type_parameter_rib(HasTypeParameters(generics, ItemRibKind), |this| {
            // Dummy self type for better errors if `Self` is used in the trait path.
            this.with_self_rib(Def::SelfTy(None, None), |this| {
                // Resolve the trait reference, if necessary.
                this.with_optional_trait_ref(opt_trait_reference.as_ref(), |this, trait_id| {
                    let item_def_id = this.definitions.local_def_id(item_id);
                    this.with_self_rib(Def::SelfTy(trait_id, Some(item_def_id)), |this| {
                        if let Some(trait_ref) = opt_trait_reference.as_ref() {
                            // Resolve type arguments in trait path
                            visit::walk_trait_ref(this, trait_ref);
                        }
                        // Resolve the self type.
                        this.visit_ty(self_type);
                        // Resolve the type parameters.
                        this.visit_generics(generics);
                        this.with_current_self_type(self_type, |this| {
                            for impl_item in impl_items {
                                this.check_proc_macro_attrs(&impl_item.attrs);
                                this.resolve_visibility(&impl_item.vis);
                                match impl_item.node {
                                    ImplItemKind::Const(..) => {
                                        // If this is a trait impl, ensure the const
                                        // exists in trait
                                        this.check_trait_item(impl_item.ident,
                                                            ValueNS,
                                                            impl_item.span,
                                            |n, s| ResolutionError::ConstNotMemberOfTrait(n, s));
                                        visit::walk_impl_item(this, impl_item);
                                    }
                                    ImplItemKind::Method(ref sig, _) => {
                                        // If this is a trait impl, ensure the method
                                        // exists in trait
                                        this.check_trait_item(impl_item.ident,
                                                            ValueNS,
                                                            impl_item.span,
                                            |n, s| ResolutionError::MethodNotMemberOfTrait(n, s));

                                        // We also need a new scope for the method-
                                        // specific type parameters.
                                        let type_parameters =
                                            HasTypeParameters(&sig.generics,
                                                            MethodRibKind(!sig.decl.has_self()));
                                        this.with_type_parameter_rib(type_parameters, |this| {
                                            visit::walk_impl_item(this, impl_item);
                                        });
                                    }
                                    ImplItemKind::Type(ref ty) => {
                                        // If this is a trait impl, ensure the type
                                        // exists in trait
                                        this.check_trait_item(impl_item.ident,
                                                            TypeNS,
                                                            impl_item.span,
                                            |n, s| ResolutionError::TypeNotMemberOfTrait(n, s));

                                        this.visit_ty(ty);
                                    }
                                    ImplItemKind::Macro(_) =>
                                        panic!("unexpanded macro in resolve!"),
                                }
                            }
                        });
                    });
                });
            });
        });
    }

    fn check_trait_item<F>(&mut self, ident: Ident, ns: Namespace, span: Span, err: F)
        where F: FnOnce(Name, &str) -> ResolutionError
    {
        // If there is a TraitRef in scope for an impl, then the method must be in the
        // trait.
        if let Some((module, _)) = self.current_trait_ref {
            if self.resolve_ident_in_module(module, ident, ns, false, false, span).is_err() {
                let path = &self.current_trait_ref.as_ref().unwrap().1.path;
                resolve_error(self, span, err(ident.name, &path_names_to_string(path)));
            }
        }
    }

    fn resolve_local(&mut self, local: &Local) {
        // Resolve the type.
        walk_list!(self, visit_ty, &local.ty);

        // Resolve the initializer.
        walk_list!(self, visit_expr, &local.init);

        // Resolve the pattern.
        self.resolve_pattern(&local.pat, PatternSource::Let, &mut FxHashMap());
    }

    // build a map from pattern identifiers to binding-info's.
    // this is done hygienically. This could arise for a macro
    // that expands into an or-pattern where one 'x' was from the
    // user and one 'x' came from the macro.
    fn binding_mode_map(&mut self, pat: &Pat) -> BindingMap {
        let mut binding_map = FxHashMap();

        pat.walk(&mut |pat| {
            if let PatKind::Ident(binding_mode, ident, ref sub_pat) = pat.node {
                if sub_pat.is_some() || match self.def_map.get(&pat.id).map(|res| res.base_def()) {
                    Some(Def::Local(..)) => true,
                    _ => false,
                } {
                    let binding_info = BindingInfo { span: ident.span, binding_mode: binding_mode };
                    binding_map.insert(ident.node, binding_info);
                }
            }
            true
        });

        binding_map
    }

    // check that all of the arms in an or-pattern have exactly the
    // same set of bindings, with the same binding modes for each.
    fn check_consistent_bindings(&mut self, arm: &Arm) {
        if arm.pats.is_empty() {
            return;
        }

        let mut missing_vars = FxHashMap();
        let mut inconsistent_vars = FxHashMap();
        for (i, p) in arm.pats.iter().enumerate() {
            let map_i = self.binding_mode_map(&p);

            for (j, q) in arm.pats.iter().enumerate() {
                if i == j {
                    continue;
                }

                let map_j = self.binding_mode_map(&q);
                for (&key, &binding_i) in &map_i {
                    if map_j.len() == 0 {                   // Account for missing bindings when
                        let binding_error = missing_vars    // map_j has none.
                            .entry(key.name)
                            .or_insert(BindingError {
                                name: key.name,
                                origin: BTreeSet::new(),
                                target: BTreeSet::new(),
                            });
                        binding_error.origin.insert(binding_i.span);
                        binding_error.target.insert(q.span);
                    }
                    for (&key_j, &binding_j) in &map_j {
                        match map_i.get(&key_j) {
                            None => {  // missing binding
                                let binding_error = missing_vars
                                    .entry(key_j.name)
                                    .or_insert(BindingError {
                                        name: key_j.name,
                                        origin: BTreeSet::new(),
                                        target: BTreeSet::new(),
                                    });
                                binding_error.origin.insert(binding_j.span);
                                binding_error.target.insert(p.span);
                            }
                            Some(binding_i) => {  // check consistent binding
                                if binding_i.binding_mode != binding_j.binding_mode {
                                    inconsistent_vars
                                        .entry(key.name)
                                        .or_insert((binding_j.span, binding_i.span));
                                }
                            }
                        }
                    }
                }
            }
        }
        let mut missing_vars = missing_vars.iter().collect::<Vec<_>>();
        missing_vars.sort();
        for (_, v) in missing_vars {
            resolve_error(self,
                          *v.origin.iter().next().unwrap(),
                          ResolutionError::VariableNotBoundInPattern(v));
        }
        let mut inconsistent_vars = inconsistent_vars.iter().collect::<Vec<_>>();
        inconsistent_vars.sort();
        for (name, v) in inconsistent_vars {
            resolve_error(self, v.0, ResolutionError::VariableBoundWithDifferentMode(*name, v.1));
        }
    }

    fn resolve_arm(&mut self, arm: &Arm) {
        self.ribs[ValueNS].push(Rib::new(NormalRibKind));

        let mut bindings_list = FxHashMap();
        for pattern in &arm.pats {
            self.resolve_pattern(&pattern, PatternSource::Match, &mut bindings_list);
        }

        // This has to happen *after* we determine which
        // pat_idents are variants
        self.check_consistent_bindings(arm);

        walk_list!(self, visit_expr, &arm.guard);
        self.visit_expr(&arm.body);

        self.ribs[ValueNS].pop();
    }

    fn resolve_block(&mut self, block: &Block) {
        debug!("(resolving block) entering block");
        // Move down in the graph, if there's an anonymous module rooted here.
        let orig_module = self.current_module;
        let anonymous_module = self.block_map.get(&block.id).cloned(); // clones a reference

        let mut num_macro_definition_ribs = 0;
        if let Some(anonymous_module) = anonymous_module {
            debug!("(resolving block) found anonymous module, moving down");
            self.ribs[ValueNS].push(Rib::new(ModuleRibKind(anonymous_module)));
            self.ribs[TypeNS].push(Rib::new(ModuleRibKind(anonymous_module)));
            self.current_module = anonymous_module;
            self.finalize_current_module_macro_resolutions();
        } else {
            self.ribs[ValueNS].push(Rib::new(NormalRibKind));
        }

        // Descend into the block.
        for stmt in &block.stmts {
            if let ast::StmtKind::Item(ref item) = stmt.node {
                if let ast::ItemKind::MacroDef(..) = item.node {
                    num_macro_definition_ribs += 1;
                    let def = self.definitions.local_def_id(item.id);
                    self.ribs[ValueNS].push(Rib::new(MacroDefinition(def)));
                    self.label_ribs.push(Rib::new(MacroDefinition(def)));
                }
            }

            self.visit_stmt(stmt);
        }

        // Move back up.
        self.current_module = orig_module;
        for _ in 0 .. num_macro_definition_ribs {
            self.ribs[ValueNS].pop();
            self.label_ribs.pop();
        }
        self.ribs[ValueNS].pop();
        if let Some(_) = anonymous_module {
            self.ribs[TypeNS].pop();
        }
        debug!("(resolving block) leaving block");
    }

    fn fresh_binding(&mut self,
                     ident: &SpannedIdent,
                     pat_id: NodeId,
                     outer_pat_id: NodeId,
                     pat_src: PatternSource,
                     bindings: &mut FxHashMap<Ident, NodeId>)
                     -> PathResolution {
        // Add the binding to the local ribs, if it
        // doesn't already exist in the bindings map. (We
        // must not add it if it's in the bindings map
        // because that breaks the assumptions later
        // passes make about or-patterns.)
        let mut def = Def::Local(self.definitions.local_def_id(pat_id));
        match bindings.get(&ident.node).cloned() {
            Some(id) if id == outer_pat_id => {
                // `Variant(a, a)`, error
                resolve_error(
                    self,
                    ident.span,
                    ResolutionError::IdentifierBoundMoreThanOnceInSamePattern(
                        &ident.node.name.as_str())
                );
            }
            Some(..) if pat_src == PatternSource::FnParam => {
                // `fn f(a: u8, a: u8)`, error
                resolve_error(
                    self,
                    ident.span,
                    ResolutionError::IdentifierBoundMoreThanOnceInParameterList(
                        &ident.node.name.as_str())
                );
            }
            Some(..) if pat_src == PatternSource::Match => {
                // `Variant1(a) | Variant2(a)`, ok
                // Reuse definition from the first `a`.
                def = self.ribs[ValueNS].last_mut().unwrap().bindings[&ident.node];
            }
            Some(..) => {
                span_bug!(ident.span, "two bindings with the same name from \
                                       unexpected pattern source {:?}", pat_src);
            }
            None => {
                // A completely fresh binding, add to the lists if it's valid.
                if ident.node.name != keywords::Invalid.name() {
                    bindings.insert(ident.node, outer_pat_id);
                    self.ribs[ValueNS].last_mut().unwrap().bindings.insert(ident.node, def);
                }
            }
        }

        PathResolution::new(def)
    }

    fn resolve_pattern(&mut self,
                       pat: &Pat,
                       pat_src: PatternSource,
                       // Maps idents to the node ID for the
                       // outermost pattern that binds them.
                       bindings: &mut FxHashMap<Ident, NodeId>) {
        // Visit all direct subpatterns of this pattern.
        let outer_pat_id = pat.id;
        pat.walk(&mut |pat| {
            match pat.node {
                PatKind::Ident(bmode, ref ident, ref opt_pat) => {
                    // First try to resolve the identifier as some existing
                    // entity, then fall back to a fresh binding.
                    let binding = self.resolve_ident_in_lexical_scope(ident.node, ValueNS,
                                                                      false, pat.span)
                                      .and_then(LexicalScopeBinding::item);
                    let resolution = binding.map(NameBinding::def).and_then(|def| {
                        let ivmode = BindingMode::ByValue(Mutability::Immutable);
                        let always_binding = !pat_src.is_refutable() || opt_pat.is_some() ||
                                             bmode != ivmode;
                        match def {
                            Def::StructCtor(_, CtorKind::Const) |
                            Def::VariantCtor(_, CtorKind::Const) |
                            Def::Const(..) if !always_binding => {
                                // A unit struct/variant or constant pattern.
                                self.record_use(ident.node, ValueNS, binding.unwrap(), ident.span);
                                Some(PathResolution::new(def))
                            }
                            Def::StructCtor(..) | Def::VariantCtor(..) |
                            Def::Const(..) | Def::Static(..) => {
                                // A fresh binding that shadows something unacceptable.
                                resolve_error(
                                    self,
                                    ident.span,
                                    ResolutionError::BindingShadowsSomethingUnacceptable(
                                        pat_src.descr(), ident.node.name, binding.unwrap())
                                );
                                None
                            }
                            Def::Local(..) | Def::Upvar(..) | Def::Fn(..) | Def::Err => {
                                // These entities are explicitly allowed
                                // to be shadowed by fresh bindings.
                                None
                            }
                            def => {
                                span_bug!(ident.span, "unexpected definition for an \
                                                       identifier in pattern: {:?}", def);
                            }
                        }
                    }).unwrap_or_else(|| {
                        self.fresh_binding(ident, pat.id, outer_pat_id, pat_src, bindings)
                    });

                    self.record_def(pat.id, resolution);
                }

                PatKind::TupleStruct(ref path, ..) => {
                    self.smart_resolve_path(pat.id, None, path, PathSource::TupleStruct);
                }

                PatKind::Path(ref qself, ref path) => {
                    self.smart_resolve_path(pat.id, qself.as_ref(), path, PathSource::Pat);
                }

                PatKind::Struct(ref path, ..) => {
                    self.smart_resolve_path(pat.id, None, path, PathSource::Struct);
                }

                _ => {}
            }
            true
        });

        visit::walk_pat(self, pat);
    }

    // High-level and context dependent path resolution routine.
    // Resolves the path and records the resolution into definition map.
    // If resolution fails tries several techniques to find likely
    // resolution candidates, suggest imports or other help, and report
    // errors in user friendly way.
    fn smart_resolve_path(&mut self,
                          id: NodeId,
                          qself: Option<&QSelf>,
                          path: &Path,
                          source: PathSource)
                          -> PathResolution {
        let segments = &path.segments.iter()
            .map(|seg| respan(seg.span, seg.identifier))
            .collect::<Vec<_>>();
        let ident_span = path.segments.last().map_or(path.span, |seg| seg.span);
        self.smart_resolve_path_fragment(id, qself, segments, path.span, ident_span, source)
    }

    fn smart_resolve_path_fragment(&mut self,
                                   id: NodeId,
                                   qself: Option<&QSelf>,
                                   path: &[SpannedIdent],
                                   span: Span,
                                   ident_span: Span,
                                   source: PathSource)
                                   -> PathResolution {
        let ns = source.namespace();
        let is_expected = &|def| source.is_expected(def);
        let is_enum_variant = &|def| if let Def::Variant(..) = def { true } else { false };

        // Base error is amended with one short label and possibly some longer helps/notes.
        let report_errors = |this: &mut Self, def: Option<Def>| {
            // Make the base error.
            let expected = source.descr_expected();
            let path_str = names_to_string(path);
            let code = source.error_code(def.is_some());
            let (base_msg, fallback_label, base_span) = if let Some(def) = def {
                (format!("expected {}, found {} `{}`", expected, def.kind_name(), path_str),
                 format!("not a {}", expected), span)
            } else {
                let item_str = path[path.len() - 1].node;
                let item_span = path[path.len() - 1].span;
                let (mod_prefix, mod_str) = if path.len() == 1 {
                    (format!(""), format!("this scope"))
                } else if path.len() == 2 && path[0].node.name == keywords::CrateRoot.name() {
                    (format!(""), format!("the crate root"))
                } else {
                    let mod_path = &path[..path.len() - 1];
                    let mod_prefix = match this.resolve_path(mod_path, Some(TypeNS), false, span) {
                        PathResult::Module(module) => module.def(),
                        _ => None,
                    }.map_or(format!(""), |def| format!("{} ", def.kind_name()));
                    (mod_prefix, format!("`{}`", names_to_string(mod_path)))
                };
                (format!("cannot find {} `{}` in {}{}", expected, item_str, mod_prefix, mod_str),
                 format!("not found in {}", mod_str), item_span)
            };
            let mut err = this.session.struct_span_err_with_code(base_span, &base_msg, code);

            // Emit special messages for unresolved `Self` and `self`.
            if is_self_type(path, ns) {
                __diagnostic_used!(E0411);
                err.code("E0411".into());
                err.span_label(span, "`Self` is only available in traits and impls");
                return err;
            }
            if is_self_value(path, ns) {
                __diagnostic_used!(E0424);
                err.code("E0424".into());
                err.span_label(span, format!("`self` value is only available in \
                                               methods with `self` parameter"));
                return err;
            }

            // Try to lookup the name in more relaxed fashion for better error reporting.
            let ident = *path.last().unwrap();
            let candidates = this.lookup_import_candidates(ident.node.name, ns, is_expected);
            if !candidates.is_empty() {
                let mut module_span = this.current_module.span;
                module_span.hi = module_span.lo;
                // Report import candidates as help and proceed searching for labels.
                show_candidates(&mut err, module_span, &candidates, def.is_some());
            } else if is_expected(Def::Enum(DefId::local(CRATE_DEF_INDEX))) {
                let enum_candidates =
                    this.lookup_import_candidates(ident.node.name, ns, is_enum_variant);
                let mut enum_candidates = enum_candidates.iter()
                    .map(|suggestion| import_candidate_to_paths(&suggestion)).collect::<Vec<_>>();
                enum_candidates.sort();
                for (sp, variant_path, enum_path) in enum_candidates {
                    if sp == DUMMY_SP {
                        let msg = format!("there is an enum variant `{}`, \
                                        try using `{}`?",
                                        variant_path,
                                        enum_path);
                        err.help(&msg);
                    } else {
                        err.span_suggestion(span, "you can try using the variant's enum",
                                            enum_path);
                    }
                }
            }
            if path.len() == 1 && this.self_type_is_available(span) {
                if let Some(candidate) = this.lookup_assoc_candidate(ident.node, ns, is_expected) {
                    let self_is_available = this.self_value_is_available(path[0].node.ctxt, span);
                    match candidate {
                        AssocSuggestion::Field => {
                            err.span_suggestion(span, "try",
                                                format!("self.{}", path_str));
                            if !self_is_available {
                                err.span_label(span, format!("`self` value is only available in \
                                                               methods with `self` parameter"));
                            }
                        }
                        AssocSuggestion::MethodWithSelf if self_is_available => {
                            err.span_suggestion(span, "try",
                                                format!("self.{}", path_str));
                        }
                        AssocSuggestion::MethodWithSelf | AssocSuggestion::AssocItem => {
                            err.span_suggestion(span, "try",
                                                format!("Self::{}", path_str));
                        }
                    }
                    return err;
                }
            }

            let mut levenshtein_worked = false;

            // Try Levenshtein.
            if let Some(candidate) = this.lookup_typo_candidate(path, ns, is_expected, span) {
                err.span_label(ident_span, format!("did you mean `{}`?", candidate));
                levenshtein_worked = true;
            }

            // Try context dependent help if relaxed lookup didn't work.
            if let Some(def) = def {
                match (def, source) {
                    (Def::Macro(..), _) => {
                        err.span_label(span, format!("did you mean `{}!(...)`?", path_str));
                        return err;
                    }
                    (Def::TyAlias(..), PathSource::Trait) => {
                        err.span_label(span, "type aliases cannot be used for traits");
                        return err;
                    }
                    (Def::Mod(..), PathSource::Expr(Some(parent))) => match parent.node {
                        ExprKind::Field(_, ident) => {
                            err.span_label(parent.span, format!("did you mean `{}::{}`?",
                                                                 path_str, ident.node));
                            return err;
                        }
                        ExprKind::MethodCall(ref segment, ..) => {
                            err.span_label(parent.span, format!("did you mean `{}::{}(...)`?",
                                                                 path_str, segment.identifier));
                            return err;
                        }
                        _ => {}
                    },
                    _ if ns == ValueNS && is_struct_like(def) => {
                        if let Def::Struct(def_id) = def {
                            if let Some((ctor_def, ctor_vis))
                                    = this.struct_constructors.get(&def_id).cloned() {
                                if is_expected(ctor_def) && !this.is_accessible(ctor_vis) {
                                    err.span_label(span, format!("constructor is not visible \
                                                                   here due to private fields"));
                                }
                            }
                        }
                        err.span_label(span, format!("did you mean `{} {{ /* fields */ }}`?",
                                                       path_str));
                        return err;
                    }
                    _ => {}
                }
            }

            // Fallback label.
            if !levenshtein_worked {
                err.span_label(base_span, fallback_label);
                this.type_ascription_suggestion(&mut err, base_span);
            }
            err
        };
        let report_errors = |this: &mut Self, def: Option<Def>| {
            report_errors(this, def).emit();
            err_path_resolution()
        };

        let resolution = match self.resolve_qpath_anywhere(id, qself, path, ns, span,
                                                           source.defer_to_typeck(),
                                                           source.global_by_default()) {
            Some(resolution) if resolution.unresolved_segments() == 0 => {
                if is_expected(resolution.base_def()) || resolution.base_def() == Def::Err {
                    resolution
                } else {
                    // Add a temporary hack to smooth the transition to new struct ctor
                    // visibility rules. See #38932 for more details.
                    let mut res = None;
                    if let Def::Struct(def_id) = resolution.base_def() {
                        if let Some((ctor_def, ctor_vis))
                                = self.struct_constructors.get(&def_id).cloned() {
                            if is_expected(ctor_def) && self.is_accessible(ctor_vis) {
                                let lint = lint::builtin::LEGACY_CONSTRUCTOR_VISIBILITY;
                                self.session.add_lint(lint, id, span,
                                    "private struct constructors are not usable through \
                                     reexports in outer modules".to_string());
                                res = Some(PathResolution::new(ctor_def));
                            }
                        }
                    }

                    res.unwrap_or_else(|| report_errors(self, Some(resolution.base_def())))
                }
            }
            Some(resolution) if source.defer_to_typeck() => {
                // Not fully resolved associated item `T::A::B` or `<T as Tr>::A::B`
                // or `<T>::A::B`. If `B` should be resolved in value namespace then
                // it needs to be added to the trait map.
                if ns == ValueNS {
                    let item_name = path.last().unwrap().node;
                    let traits = self.get_traits_containing_item(item_name, ns);
                    self.trait_map.insert(id, traits);
                }
                resolution
            }
            _ => report_errors(self, None)
        };

        if let PathSource::TraitItem(..) = source {} else {
            // Avoid recording definition of `A::B` in `<T as A>::B::C`.
            self.record_def(id, resolution);
        }
        resolution
    }

    fn type_ascription_suggestion(&self,
                                  err: &mut DiagnosticBuilder,
                                  base_span: Span) {
        debug!("type_ascription_suggetion {:?}", base_span);
        let cm = self.session.codemap();
        debug!("self.current_type_ascription {:?}", self.current_type_ascription);
        if let Some(sp) = self.current_type_ascription.last() {
            let mut sp = *sp;
            loop {  // try to find the `:`, bail on first non-':'/non-whitespace
                sp = sp.next_point();
                if let Ok(snippet) = cm.span_to_snippet(sp.to(sp.next_point())) {
                    debug!("snippet {:?}", snippet);
                    let line_sp = cm.lookup_char_pos(sp.hi).line;
                    let line_base_sp = cm.lookup_char_pos(base_span.lo).line;
                    debug!("{:?} {:?}", line_sp, line_base_sp);
                    if snippet == ":" {
                        err.span_label(base_span,
                                       "expecting a type here because of type ascription");
                        if line_sp != line_base_sp {
                            err.span_suggestion_short(sp,
                                                      "did you mean to use `;` here instead?",
                                                      ";".to_string());
                        }
                        break;
                    } else if snippet.trim().len() != 0  {
                        debug!("tried to find type ascription `:` token, couldn't find it");
                        break;
                    }
                } else {
                    break;
                }
            }
        }
    }

    fn self_type_is_available(&mut self, span: Span) -> bool {
        let binding = self.resolve_ident_in_lexical_scope(keywords::SelfType.ident(),
                                                          TypeNS, false, span);
        if let Some(LexicalScopeBinding::Def(def)) = binding { def != Def::Err } else { false }
    }

    fn self_value_is_available(&mut self, ctxt: SyntaxContext, span: Span) -> bool {
        let ident = Ident { name: keywords::SelfValue.name(), ctxt: ctxt };
        let binding = self.resolve_ident_in_lexical_scope(ident, ValueNS, false, span);
        if let Some(LexicalScopeBinding::Def(def)) = binding { def != Def::Err } else { false }
    }

    // Resolve in alternative namespaces if resolution in the primary namespace fails.
    fn resolve_qpath_anywhere(&mut self,
                              id: NodeId,
                              qself: Option<&QSelf>,
                              path: &[SpannedIdent],
                              primary_ns: Namespace,
                              span: Span,
                              defer_to_typeck: bool,
                              global_by_default: bool)
                              -> Option<PathResolution> {
        let mut fin_res = None;
        // FIXME: can't resolve paths in macro namespace yet, macros are
        // processed by the little special hack below.
        for (i, ns) in [primary_ns, TypeNS, ValueNS, /*MacroNS*/].iter().cloned().enumerate() {
            if i == 0 || ns != primary_ns {
                match self.resolve_qpath(id, qself, path, ns, span, global_by_default) {
                    // If defer_to_typeck, then resolution > no resolution,
                    // otherwise full resolution > partial resolution > no resolution.
                    Some(res) if res.unresolved_segments() == 0 || defer_to_typeck =>
                        return Some(res),
                    res => if fin_res.is_none() { fin_res = res },
                };
            }
        }
        let is_global = self.global_macros.get(&path[0].node.name).cloned()
            .map(|binding| binding.get_macro(self).kind() == MacroKind::Bang).unwrap_or(false);
        if primary_ns != MacroNS && (is_global ||
                                     self.macro_names.contains(&path[0].node.modern())) {
            // Return some dummy definition, it's enough for error reporting.
            return Some(
                PathResolution::new(Def::Macro(DefId::local(CRATE_DEF_INDEX), MacroKind::Bang))
            );
        }
        fin_res
    }

    /// Handles paths that may refer to associated items.
    fn resolve_qpath(&mut self,
                     id: NodeId,
                     qself: Option<&QSelf>,
                     path: &[SpannedIdent],
                     ns: Namespace,
                     span: Span,
                     global_by_default: bool)
                     -> Option<PathResolution> {
        if let Some(qself) = qself {
            if qself.position == 0 {
                // FIXME: Create some fake resolution that can't possibly be a type.
                return Some(PathResolution::with_unresolved_segments(
                    Def::Mod(DefId::local(CRATE_DEF_INDEX)), path.len()
                ));
            }
            // Make sure `A::B` in `<T as A>::B::C` is a trait item.
            let ns = if qself.position + 1 == path.len() { ns } else { TypeNS };
            let res = self.smart_resolve_path_fragment(id, None, &path[..qself.position + 1],
                                                       span, span, PathSource::TraitItem(ns));
            return Some(PathResolution::with_unresolved_segments(
                res.base_def(), res.unresolved_segments() + path.len() - qself.position - 1
            ));
        }

        let result = match self.resolve_path(&path, Some(ns), true, span) {
            PathResult::NonModule(path_res) => path_res,
            PathResult::Module(module) if !module.is_normal() => {
                PathResolution::new(module.def().unwrap())
            }
            // In `a(::assoc_item)*` `a` cannot be a module. If `a` does resolve to a module we
            // don't report an error right away, but try to fallback to a primitive type.
            // So, we are still able to successfully resolve something like
            //
            // use std::u8; // bring module u8 in scope
            // fn f() -> u8 { // OK, resolves to primitive u8, not to std::u8
            //     u8::max_value() // OK, resolves to associated function <u8>::max_value,
            //                     // not to non-existent std::u8::max_value
            // }
            //
            // Such behavior is required for backward compatibility.
            // The same fallback is used when `a` resolves to nothing.
            PathResult::Module(..) | PathResult::Failed(..)
                    if (ns == TypeNS || path.len() > 1) &&
                       self.primitive_type_table.primitive_types
                           .contains_key(&path[0].node.name) => {
                let prim = self.primitive_type_table.primitive_types[&path[0].node.name];
                match prim {
                    TyUint(UintTy::U128) | TyInt(IntTy::I128) => {
                        if !self.session.features.borrow().i128_type {
                            emit_feature_err(&self.session.parse_sess,
                                                "i128_type", span, GateIssue::Language,
                                                "128-bit type is unstable");

                        }
                    }
                    _ => {}
                }
                PathResolution::with_unresolved_segments(Def::PrimTy(prim), path.len() - 1)
            }
            PathResult::Module(module) => PathResolution::new(module.def().unwrap()),
            PathResult::Failed(span, msg, false) => {
                resolve_error(self, span, ResolutionError::FailedToResolve(&msg));
                err_path_resolution()
            }
            PathResult::Failed(..) => return None,
            PathResult::Indeterminate => bug!("indetermined path result in resolve_qpath"),
        };

        if path.len() > 1 && !global_by_default && result.base_def() != Def::Err &&
           path[0].node.name != keywords::CrateRoot.name() &&
           path[0].node.name != keywords::DollarCrate.name() {
            let unqualified_result = {
                match self.resolve_path(&[*path.last().unwrap()], Some(ns), false, span) {
                    PathResult::NonModule(path_res) => path_res.base_def(),
                    PathResult::Module(module) => module.def().unwrap(),
                    _ => return Some(result),
                }
            };
            if result.base_def() == unqualified_result {
                let lint = lint::builtin::UNUSED_QUALIFICATIONS;
                self.session.add_lint(lint, id, span, "unnecessary qualification".to_string());
            }
        }

        Some(result)
    }

    fn resolve_path(&mut self,
                    path: &[SpannedIdent],
                    opt_ns: Option<Namespace>, // `None` indicates a module path
                    record_used: bool,
                    path_span: Span)
                    -> PathResult<'a> {
        let mut module = None;
        let mut allow_super = true;

        for (i, &ident) in path.iter().enumerate() {
            debug!("resolve_path ident {} {:?}", i, ident);
            let is_last = i == path.len() - 1;
            let ns = if is_last { opt_ns.unwrap_or(TypeNS) } else { TypeNS };

            if i == 0 && ns == TypeNS && ident.node.name == keywords::SelfValue.name() {
                let mut ctxt = ident.node.ctxt.modern();
                module = Some(self.resolve_self(&mut ctxt, self.current_module));
                continue
            } else if allow_super && ns == TypeNS && ident.node.name == keywords::Super.name() {
                let mut ctxt = ident.node.ctxt.modern();
                let self_module = match i {
                    0 => self.resolve_self(&mut ctxt, self.current_module),
                    _ => module.unwrap(),
                };
                if let Some(parent) = self_module.parent {
                    module = Some(self.resolve_self(&mut ctxt, parent));
                    continue
                } else {
                    let msg = "There are too many initial `super`s.".to_string();
                    return PathResult::Failed(ident.span, msg, false);
                }
            }
            allow_super = false;

            if i == 0 && ns == TypeNS && ident.node.name == keywords::CrateRoot.name() {
                module = Some(self.resolve_crate_root(ident.node.ctxt.modern()));
                continue
            } else if i == 0 && ns == TypeNS && ident.node.name == keywords::DollarCrate.name() {
                module = Some(self.resolve_crate_root(ident.node.ctxt));
                continue
            }

            let binding = if let Some(module) = module {
                self.resolve_ident_in_module(module, ident.node, ns, false, record_used, path_span)
            } else if opt_ns == Some(MacroNS) {
                self.resolve_lexical_macro_path_segment(ident.node, ns, record_used, path_span)
                    .map(MacroBinding::binding)
            } else {
                match self.resolve_ident_in_lexical_scope(ident.node, ns, record_used, path_span) {
                    Some(LexicalScopeBinding::Item(binding)) => Ok(binding),
                    Some(LexicalScopeBinding::Def(def))
                            if opt_ns == Some(TypeNS) || opt_ns == Some(ValueNS) => {
                        return PathResult::NonModule(PathResolution::with_unresolved_segments(
                            def, path.len() - 1
                        ));
                    }
                    _ => Err(if record_used { Determined } else { Undetermined }),
                }
            };

            match binding {
                Ok(binding) => {
                    let def = binding.def();
                    let maybe_assoc = opt_ns != Some(MacroNS) && PathSource::Type.is_expected(def);
                    if let Some(next_module) = binding.module() {
                        module = Some(next_module);
                    } else if def == Def::Err {
                        return PathResult::NonModule(err_path_resolution());
                    } else if opt_ns.is_some() && (is_last || maybe_assoc) {
                        return PathResult::NonModule(PathResolution::with_unresolved_segments(
                            def, path.len() - i - 1
                        ));
                    } else {
                        return PathResult::Failed(ident.span,
                                                  format!("Not a module `{}`", ident.node),
                                                  is_last);
                    }
                }
                Err(Undetermined) => return PathResult::Indeterminate,
                Err(Determined) => {
                    if let Some(module) = module {
                        if opt_ns.is_some() && !module.is_normal() {
                            return PathResult::NonModule(PathResolution::with_unresolved_segments(
                                module.def().unwrap(), path.len() - i
                            ));
                        }
                    }
                    let msg = if module.and_then(ModuleData::def) == self.graph_root.def() {
                        let is_mod = |def| match def { Def::Mod(..) => true, _ => false };
                        let mut candidates =
                            self.lookup_import_candidates(ident.node.name, TypeNS, is_mod);
                        candidates.sort_by_key(|c| (c.path.segments.len(), c.path.to_string()));
                        if let Some(candidate) = candidates.get(0) {
                            format!("Did you mean `{}`?", candidate.path)
                        } else {
                            format!("Maybe a missing `extern crate {};`?", ident.node)
                        }
                    } else if i == 0 {
                        format!("Use of undeclared type or module `{}`", ident.node)
                    } else {
                        format!("Could not find `{}` in `{}`", ident.node, path[i - 1].node)
                    };
                    return PathResult::Failed(ident.span, msg, is_last);
                }
            }
        }

        PathResult::Module(module.unwrap_or(self.graph_root))
    }

    // Resolve a local definition, potentially adjusting for closures.
    fn adjust_local_def(&mut self,
                        ns: Namespace,
                        rib_index: usize,
                        mut def: Def,
                        record_used: bool,
                        span: Span) -> Def {
        let ribs = &self.ribs[ns][rib_index + 1..];

        // An invalid forward use of a type parameter from a previous default.
        if let ForwardTyParamBanRibKind = self.ribs[ns][rib_index].kind {
            if record_used {
                resolve_error(self, span, ResolutionError::ForwardDeclaredTyParam);
            }
            assert_eq!(def, Def::Err);
            return Def::Err;
        }

        match def {
            Def::Upvar(..) => {
                span_bug!(span, "unexpected {:?} in bindings", def)
            }
            Def::Local(def_id) => {
                for rib in ribs {
                    match rib.kind {
                        NormalRibKind | ModuleRibKind(..) | MacroDefinition(..) |
                        ForwardTyParamBanRibKind => {
                            // Nothing to do. Continue.
                        }
                        ClosureRibKind(function_id) => {
                            let prev_def = def;
                            let node_id = self.definitions.as_local_node_id(def_id).unwrap();

                            let seen = self.freevars_seen
                                           .entry(function_id)
                                           .or_insert_with(|| NodeMap());
                            if let Some(&index) = seen.get(&node_id) {
                                def = Def::Upvar(def_id, index, function_id);
                                continue;
                            }
                            let vec = self.freevars
                                          .entry(function_id)
                                          .or_insert_with(|| vec![]);
                            let depth = vec.len();
                            def = Def::Upvar(def_id, depth, function_id);

                            if record_used {
                                vec.push(Freevar {
                                    def: prev_def,
                                    span: span,
                                });
                                seen.insert(node_id, depth);
                            }
                        }
                        ItemRibKind | MethodRibKind(_) => {
                            // This was an attempt to access an upvar inside a
                            // named function item. This is not allowed, so we
                            // report an error.
                            if record_used {
                                resolve_error(self, span,
                                        ResolutionError::CannotCaptureDynamicEnvironmentInFnItem);
                            }
                            return Def::Err;
                        }
                        ConstantItemRibKind => {
                            // Still doesn't deal with upvars
                            if record_used {
                                resolve_error(self, span,
                                        ResolutionError::AttemptToUseNonConstantValueInConstant);
                            }
                            return Def::Err;
                        }
                    }
                }
            }
            Def::TyParam(..) | Def::SelfTy(..) => {
                for rib in ribs {
                    match rib.kind {
                        NormalRibKind | MethodRibKind(_) | ClosureRibKind(..) |
                        ModuleRibKind(..) | MacroDefinition(..) | ForwardTyParamBanRibKind |
                        ConstantItemRibKind => {
                            // Nothing to do. Continue.
                        }
                        ItemRibKind => {
                            // This was an attempt to use a type parameter outside
                            // its scope.
                            if record_used {
                                resolve_error(self, span,
                                              ResolutionError::TypeParametersFromOuterFunction);
                            }
                            return Def::Err;
                        }
                    }
                }
            }
            _ => {}
        }
        return def;
    }

    // Calls `f` with a `Resolver` whose current lexical scope is `module`'s lexical scope,
    // i.e. the module's items and the prelude (unless the module is `#[no_implicit_prelude]`).
    // FIXME #34673: This needs testing.
    pub fn with_module_lexical_scope<T, F>(&mut self, module: Module<'a>, f: F) -> T
        where F: FnOnce(&mut Resolver<'a>) -> T,
    {
        self.with_empty_ribs(|this| {
            this.ribs[ValueNS].push(Rib::new(ModuleRibKind(module)));
            this.ribs[TypeNS].push(Rib::new(ModuleRibKind(module)));
            f(this)
        })
    }

    fn with_empty_ribs<T, F>(&mut self, f: F) -> T
        where F: FnOnce(&mut Resolver<'a>) -> T,
    {
        let ribs = replace(&mut self.ribs, PerNS::<Vec<Rib>>::default());
        let label_ribs = replace(&mut self.label_ribs, Vec::new());

        let result = f(self);
        self.ribs = ribs;
        self.label_ribs = label_ribs;
        result
    }

    fn lookup_assoc_candidate<FilterFn>(&mut self,
                                        ident: Ident,
                                        ns: Namespace,
                                        filter_fn: FilterFn)
                                        -> Option<AssocSuggestion>
        where FilterFn: Fn(Def) -> bool
    {
        fn extract_node_id(t: &Ty) -> Option<NodeId> {
            match t.node {
                TyKind::Path(None, _) => Some(t.id),
                TyKind::Rptr(_, ref mut_ty) => extract_node_id(&mut_ty.ty),
                // This doesn't handle the remaining `Ty` variants as they are not
                // that commonly the self_type, it might be interesting to provide
                // support for those in future.
                _ => None,
            }
        }

        // Fields are generally expected in the same contexts as locals.
        if filter_fn(Def::Local(DefId::local(CRATE_DEF_INDEX))) {
            if let Some(node_id) = self.current_self_type.as_ref().and_then(extract_node_id) {
                // Look for a field with the same name in the current self_type.
                if let Some(resolution) = self.def_map.get(&node_id) {
                    match resolution.base_def() {
                        Def::Struct(did) | Def::Union(did)
                                if resolution.unresolved_segments() == 0 => {
                            if let Some(field_names) = self.field_names.get(&did) {
                                if field_names.iter().any(|&field_name| ident.name == field_name) {
                                    return Some(AssocSuggestion::Field);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Look for associated items in the current trait.
        if let Some((module, _)) = self.current_trait_ref {
            if let Ok(binding) =
                    self.resolve_ident_in_module(module, ident, ns, false, false, module.span) {
                let def = binding.def();
                if filter_fn(def) {
                    return Some(if self.has_self.contains(&def.def_id()) {
                        AssocSuggestion::MethodWithSelf
                    } else {
                        AssocSuggestion::AssocItem
                    });
                }
            }
        }

        None
    }

    fn lookup_typo_candidate<FilterFn>(&mut self,
                                       path: &[SpannedIdent],
                                       ns: Namespace,
                                       filter_fn: FilterFn,
                                       span: Span)
                                       -> Option<Symbol>
        where FilterFn: Fn(Def) -> bool
    {
        let add_module_candidates = |module: Module, names: &mut Vec<Name>| {
            for (&(ident, _), resolution) in module.resolutions.borrow().iter() {
                if let Some(binding) = resolution.borrow().binding {
                    if filter_fn(binding.def()) {
                        names.push(ident.name);
                    }
                }
            }
        };

        let mut names = Vec::new();
        if path.len() == 1 {
            // Search in lexical scope.
            // Walk backwards up the ribs in scope and collect candidates.
            for rib in self.ribs[ns].iter().rev() {
                // Locals and type parameters
                for (ident, def) in &rib.bindings {
                    if filter_fn(*def) {
                        names.push(ident.name);
                    }
                }
                // Items in scope
                if let ModuleRibKind(module) = rib.kind {
                    // Items from this module
                    add_module_candidates(module, &mut names);

                    if let ModuleKind::Block(..) = module.kind {
                        // We can see through blocks
                    } else {
                        // Items from the prelude
                        if let Some(prelude) = self.prelude {
                            if !module.no_implicit_prelude {
                                add_module_candidates(prelude, &mut names);
                            }
                        }
                        break;
                    }
                }
            }
            // Add primitive types to the mix
            if filter_fn(Def::PrimTy(TyBool)) {
                for (name, _) in &self.primitive_type_table.primitive_types {
                    names.push(*name);
                }
            }
        } else {
            // Search in module.
            let mod_path = &path[..path.len() - 1];
            if let PathResult::Module(module) = self.resolve_path(mod_path, Some(TypeNS),
                                                                  false, span) {
                add_module_candidates(module, &mut names);
            }
        }

        let name = path[path.len() - 1].node.name;
        // Make sure error reporting is deterministic.
        names.sort_by_key(|name| name.as_str());
        match find_best_match_for_name(names.iter(), &name.as_str(), None) {
            Some(found) if found != name => Some(found),
            _ => None,
        }
    }

    fn with_resolved_label<F>(&mut self, label: Option<SpannedIdent>, id: NodeId, f: F)
        where F: FnOnce(&mut Resolver)
    {
        if let Some(label) = label {
            let def = Def::Label(id);
            self.with_label_rib(|this| {
                this.label_ribs.last_mut().unwrap().bindings.insert(label.node, def);
                f(this);
            });
        } else {
            f(self);
        }
    }

    fn resolve_labeled_block(&mut self, label: Option<SpannedIdent>, id: NodeId, block: &Block) {
        self.with_resolved_label(label, id, |this| this.visit_block(block));
    }

    fn resolve_expr(&mut self, expr: &Expr, parent: Option<&Expr>) {
        // First, record candidate traits for this expression if it could
        // result in the invocation of a method call.

        self.record_candidate_traits_for_expr_if_necessary(expr);

        // Next, resolve the node.
        match expr.node {
            ExprKind::Path(ref qself, ref path) => {
                self.smart_resolve_path(expr.id, qself.as_ref(), path, PathSource::Expr(parent));
                visit::walk_expr(self, expr);
            }

            ExprKind::Struct(ref path, ..) => {
                self.smart_resolve_path(expr.id, None, path, PathSource::Struct);
                visit::walk_expr(self, expr);
            }

            ExprKind::Break(Some(label), _) | ExprKind::Continue(Some(label)) => {
                match self.search_label(label.node) {
                    None => {
                        self.record_def(expr.id, err_path_resolution());
                        resolve_error(self,
                                      label.span,
                                      ResolutionError::UndeclaredLabel(&label.node.name.as_str()));
                    }
                    Some(def @ Def::Label(_)) => {
                        // Since this def is a label, it is never read.
                        self.record_def(expr.id, PathResolution::new(def));
                    }
                    Some(_) => {
                        span_bug!(expr.span, "label wasn't mapped to a label def!");
                    }
                }

                // visit `break` argument if any
                visit::walk_expr(self, expr);
            }

            ExprKind::IfLet(ref pattern, ref subexpression, ref if_block, ref optional_else) => {
                self.visit_expr(subexpression);

                self.ribs[ValueNS].push(Rib::new(NormalRibKind));
                self.resolve_pattern(pattern, PatternSource::IfLet, &mut FxHashMap());
                self.visit_block(if_block);
                self.ribs[ValueNS].pop();

                optional_else.as_ref().map(|expr| self.visit_expr(expr));
            }

            ExprKind::Loop(ref block, label) => self.resolve_labeled_block(label, expr.id, &block),

            ExprKind::While(ref subexpression, ref block, label) => {
                self.with_resolved_label(label, expr.id, |this| {
                    this.visit_expr(subexpression);
                    this.visit_block(block);
                });
            }

            ExprKind::WhileLet(ref pattern, ref subexpression, ref block, label) => {
                self.with_resolved_label(label, expr.id, |this| {
                    this.visit_expr(subexpression);
                    this.ribs[ValueNS].push(Rib::new(NormalRibKind));
                    this.resolve_pattern(pattern, PatternSource::WhileLet, &mut FxHashMap());
                    this.visit_block(block);
                    this.ribs[ValueNS].pop();
                });
            }

            ExprKind::ForLoop(ref pattern, ref subexpression, ref block, label) => {
                self.visit_expr(subexpression);
                self.ribs[ValueNS].push(Rib::new(NormalRibKind));
                self.resolve_pattern(pattern, PatternSource::For, &mut FxHashMap());

                self.resolve_labeled_block(label, expr.id, block);

                self.ribs[ValueNS].pop();
            }

            // Equivalent to `visit::walk_expr` + passing some context to children.
            ExprKind::Field(ref subexpression, _) => {
                self.resolve_expr(subexpression, Some(expr));
            }
            ExprKind::MethodCall(ref segment, ref arguments) => {
                let mut arguments = arguments.iter();
                self.resolve_expr(arguments.next().unwrap(), Some(expr));
                for argument in arguments {
                    self.resolve_expr(argument, None);
                }
                self.visit_path_segment(expr.span, segment);
            }

            ExprKind::Repeat(ref element, ref count) => {
                self.visit_expr(element);
                self.with_constant_rib(|this| {
                    this.visit_expr(count);
                });
            }
            ExprKind::Call(ref callee, ref arguments) => {
                self.resolve_expr(callee, Some(expr));
                for argument in arguments {
                    self.resolve_expr(argument, None);
                }
            }
            ExprKind::Type(ref type_expr, _) => {
                self.current_type_ascription.push(type_expr.span);
                visit::walk_expr(self, expr);
                self.current_type_ascription.pop();
            }
            _ => {
                visit::walk_expr(self, expr);
            }
        }
    }

    fn record_candidate_traits_for_expr_if_necessary(&mut self, expr: &Expr) {
        match expr.node {
            ExprKind::Field(_, name) => {
                // FIXME(#6890): Even though you can't treat a method like a
                // field, we need to add any trait methods we find that match
                // the field name so that we can do some nice error reporting
                // later on in typeck.
                let traits = self.get_traits_containing_item(name.node, ValueNS);
                self.trait_map.insert(expr.id, traits);
            }
            ExprKind::MethodCall(ref segment, ..) => {
                debug!("(recording candidate traits for expr) recording traits for {}",
                       expr.id);
                let traits = self.get_traits_containing_item(segment.identifier, ValueNS);
                self.trait_map.insert(expr.id, traits);
            }
            _ => {
                // Nothing to do.
            }
        }
    }

    fn get_traits_containing_item(&mut self, mut ident: Ident, ns: Namespace)
                                  -> Vec<TraitCandidate> {
        debug!("(getting traits containing item) looking for '{}'", ident.name);

        let mut found_traits = Vec::new();
        // Look for the current trait.
        if let Some((module, _)) = self.current_trait_ref {
            if self.resolve_ident_in_module(module, ident, ns, false, false, module.span).is_ok() {
                let def_id = module.def_id().unwrap();
                found_traits.push(TraitCandidate { def_id: def_id, import_id: None });
            }
        }

        ident.ctxt = ident.ctxt.modern();
        let mut search_module = self.current_module;
        loop {
            self.get_traits_in_module_containing_item(ident, ns, search_module, &mut found_traits);
            search_module =
                unwrap_or!(self.hygienic_lexical_parent(search_module, &mut ident.ctxt), break);
        }

        if let Some(prelude) = self.prelude {
            if !search_module.no_implicit_prelude {
                self.get_traits_in_module_containing_item(ident, ns, prelude, &mut found_traits);
            }
        }

        found_traits
    }

    fn get_traits_in_module_containing_item(&mut self,
                                            ident: Ident,
                                            ns: Namespace,
                                            module: Module<'a>,
                                            found_traits: &mut Vec<TraitCandidate>) {
        let mut traits = module.traits.borrow_mut();
        if traits.is_none() {
            let mut collected_traits = Vec::new();
            module.for_each_child(|name, ns, binding| {
                if ns != TypeNS { return }
                if let Def::Trait(_) = binding.def() {
                    collected_traits.push((name, binding));
                }
            });
            *traits = Some(collected_traits.into_boxed_slice());
        }

        for &(trait_name, binding) in traits.as_ref().unwrap().iter() {
            let module = binding.module().unwrap();
            let mut ident = ident;
            if ident.ctxt.glob_adjust(module.expansion, binding.span.ctxt.modern()).is_none() {
                continue
            }
            if self.resolve_ident_in_module_unadjusted(module, ident, ns, false, false, module.span)
                   .is_ok() {
                let import_id = match binding.kind {
                    NameBindingKind::Import { directive, .. } => {
                        self.maybe_unused_trait_imports.insert(directive.id);
                        self.add_to_glob_map(directive.id, trait_name);
                        Some(directive.id)
                    }
                    _ => None,
                };
                let trait_def_id = module.def_id().unwrap();
                found_traits.push(TraitCandidate { def_id: trait_def_id, import_id: import_id });
            }
        }
    }

    /// When name resolution fails, this method can be used to look up candidate
    /// entities with the expected name. It allows filtering them using the
    /// supplied predicate (which should be used to only accept the types of
    /// definitions expected e.g. traits). The lookup spans across all crates.
    ///
    /// NOTE: The method does not look into imports, but this is not a problem,
    /// since we report the definitions (thus, the de-aliased imports).
    fn lookup_import_candidates<FilterFn>(&mut self,
                                          lookup_name: Name,
                                          namespace: Namespace,
                                          filter_fn: FilterFn)
                                          -> Vec<ImportSuggestion>
        where FilterFn: Fn(Def) -> bool
    {
        let mut candidates = Vec::new();
        let mut worklist = Vec::new();
        let mut seen_modules = FxHashSet();
        worklist.push((self.graph_root, Vec::new(), false));

        while let Some((in_module,
                        path_segments,
                        in_module_is_extern)) = worklist.pop() {
            self.populate_module_if_necessary(in_module);

            // We have to visit module children in deterministic order to avoid
            // instabilities in reported imports (#43552).
            in_module.for_each_child_stable(|ident, ns, name_binding| {
                // avoid imports entirely
                if name_binding.is_import() && !name_binding.is_extern_crate() { return; }
                // avoid non-importable candidates as well
                if !name_binding.is_importable() { return; }

                // collect results based on the filter function
                if ident.name == lookup_name && ns == namespace {
                    if filter_fn(name_binding.def()) {
                        // create the path
                        let mut segms = path_segments.clone();
                        segms.push(ast::PathSegment::from_ident(ident, name_binding.span));
                        let path = Path {
                            span: name_binding.span,
                            segments: segms,
                        };
                        // the entity is accessible in the following cases:
                        // 1. if it's defined in the same crate, it's always
                        // accessible (since private entities can be made public)
                        // 2. if it's defined in another crate, it's accessible
                        // only if both the module is public and the entity is
                        // declared as public (due to pruning, we don't explore
                        // outside crate private modules => no need to check this)
                        if !in_module_is_extern || name_binding.vis == ty::Visibility::Public {
                            candidates.push(ImportSuggestion { path: path });
                        }
                    }
                }

                // collect submodules to explore
                if let Some(module) = name_binding.module() {
                    // form the path
                    let mut path_segments = path_segments.clone();
                    path_segments.push(ast::PathSegment::from_ident(ident, name_binding.span));

                    if !in_module_is_extern || name_binding.vis == ty::Visibility::Public {
                        // add the module to the lookup
                        let is_extern = in_module_is_extern || name_binding.is_extern_crate();
                        if seen_modules.insert(module.def_id().unwrap()) {
                            worklist.push((module, path_segments, is_extern));
                        }
                    }
                }
            })
        }

        candidates
    }

    fn record_def(&mut self, node_id: NodeId, resolution: PathResolution) {
        debug!("(recording def) recording {:?} for {}", resolution, node_id);
        if let Some(prev_res) = self.def_map.insert(node_id, resolution) {
            panic!("path resolved multiple times ({:?} before, {:?} now)", prev_res, resolution);
        }
    }

    fn resolve_visibility(&mut self, vis: &ast::Visibility) -> ty::Visibility {
        match *vis {
            ast::Visibility::Public => ty::Visibility::Public,
            ast::Visibility::Crate(..) => ty::Visibility::Restricted(DefId::local(CRATE_DEF_INDEX)),
            ast::Visibility::Inherited => {
                ty::Visibility::Restricted(self.current_module.normal_ancestor_id)
            }
            ast::Visibility::Restricted { ref path, id } => {
                let def = self.smart_resolve_path(id, None, path,
                                                  PathSource::Visibility).base_def();
                if def == Def::Err {
                    ty::Visibility::Public
                } else {
                    let vis = ty::Visibility::Restricted(def.def_id());
                    if self.is_accessible(vis) {
                        vis
                    } else {
                        self.session.span_err(path.span, "visibilities can only be restricted \
                                                          to ancestor modules");
                        ty::Visibility::Public
                    }
                }
            }
        }
    }

    fn is_accessible(&self, vis: ty::Visibility) -> bool {
        vis.is_accessible_from(self.current_module.normal_ancestor_id, self)
    }

    fn is_accessible_from(&self, vis: ty::Visibility, module: Module<'a>) -> bool {
        vis.is_accessible_from(module.normal_ancestor_id, self)
    }

    fn report_errors(&mut self) {
        self.report_shadowing_errors();
        let mut reported_spans = FxHashSet();

        for &AmbiguityError { span, name, b1, b2, lexical, legacy } in &self.ambiguity_errors {
            if !reported_spans.insert(span) { continue }
            let participle = |binding: &NameBinding| {
                if binding.is_import() { "imported" } else { "defined" }
            };
            let msg1 = format!("`{}` could refer to the name {} here", name, participle(b1));
            let msg2 = format!("`{}` could also refer to the name {} here", name, participle(b2));
            let note = if b1.expansion == Mark::root() || !lexical && b1.is_glob_import() {
                format!("consider adding an explicit import of `{}` to disambiguate", name)
            } else if let Def::Macro(..) = b1.def() {
                format!("macro-expanded {} do not shadow",
                        if b1.is_import() { "macro imports" } else { "macros" })
            } else {
                format!("macro-expanded {} do not shadow when used in a macro invocation path",
                        if b1.is_import() { "imports" } else { "items" })
            };
            if legacy {
                let id = match b2.kind {
                    NameBindingKind::Import { directive, .. } => directive.id,
                    _ => unreachable!(),
                };
                let mut span = MultiSpan::from_span(span);
                span.push_span_label(b1.span, msg1);
                span.push_span_label(b2.span, msg2);
                let msg = format!("`{}` is ambiguous", name);
                self.session.add_lint(lint::builtin::LEGACY_IMPORTS, id, span, msg);
            } else {
                let mut err =
                    self.session.struct_span_err(span, &format!("`{}` is ambiguous", name));
                err.span_note(b1.span, &msg1);
                match b2.def() {
                    Def::Macro(..) if b2.span == DUMMY_SP =>
                        err.note(&format!("`{}` is also a builtin macro", name)),
                    _ => err.span_note(b2.span, &msg2),
                };
                err.note(&note).emit();
            }
        }

        for &PrivacyError(span, name, binding) in &self.privacy_errors {
            if !reported_spans.insert(span) { continue }
            span_err!(self.session, span, E0603, "{} `{}` is private", binding.descr(), name);
        }
    }

    fn report_shadowing_errors(&mut self) {
        for (ident, scope) in replace(&mut self.lexical_macro_resolutions, Vec::new()) {
            self.resolve_legacy_scope(scope, ident, true);
        }

        let mut reported_errors = FxHashSet();
        for binding in replace(&mut self.disallowed_shadowing, Vec::new()) {
            if self.resolve_legacy_scope(&binding.parent, binding.ident, false).is_some() &&
               reported_errors.insert((binding.ident, binding.span)) {
                let msg = format!("`{}` is already in scope", binding.ident);
                self.session.struct_span_err(binding.span, &msg)
                    .note("macro-expanded `macro_rules!`s may not shadow \
                           existing macros (see RFC 1560)")
                    .emit();
            }
        }
    }

    fn report_conflict(&mut self,
                       parent: Module,
                       ident: Ident,
                       ns: Namespace,
                       new_binding: &NameBinding,
                       old_binding: &NameBinding) {
        // Error on the second of two conflicting names
        if old_binding.span.lo > new_binding.span.lo {
            return self.report_conflict(parent, ident, ns, old_binding, new_binding);
        }

        let container = match parent.kind {
            ModuleKind::Def(Def::Mod(_), _) => "module",
            ModuleKind::Def(Def::Trait(_), _) => "trait",
            ModuleKind::Block(..) => "block",
            _ => "enum",
        };

        let old_noun = match old_binding.is_import() {
            true => "import",
            false => "definition",
        };

        let new_participle = match new_binding.is_import() {
            true => "imported",
            false => "defined",
        };

        let (name, span) = (ident.name, new_binding.span);

        if let Some(s) = self.name_already_seen.get(&name) {
            if s == &span {
                return;
            }
        }

        let old_kind = match (ns, old_binding.module()) {
            (ValueNS, _) => "value",
            (MacroNS, _) => "macro",
            (TypeNS, _) if old_binding.is_extern_crate() => "extern crate",
            (TypeNS, Some(module)) if module.is_normal() => "module",
            (TypeNS, Some(module)) if module.is_trait() => "trait",
            (TypeNS, _) => "type",
        };

        let namespace = match ns {
            ValueNS => "value",
            MacroNS => "macro",
            TypeNS => "type",
        };

        let msg = format!("the name `{}` is defined multiple times", name);

        let mut err = match (old_binding.is_extern_crate(), new_binding.is_extern_crate()) {
            (true, true) => struct_span_err!(self.session, span, E0259, "{}", msg),
            (true, _) | (_, true) => match new_binding.is_import() && old_binding.is_import() {
                true => struct_span_err!(self.session, span, E0254, "{}", msg),
                false => struct_span_err!(self.session, span, E0260, "{}", msg),
            },
            _ => match (old_binding.is_import(), new_binding.is_import()) {
                (false, false) => struct_span_err!(self.session, span, E0428, "{}", msg),
                (true, true) => struct_span_err!(self.session, span, E0252, "{}", msg),
                _ => struct_span_err!(self.session, span, E0255, "{}", msg),
            },
        };

        err.note(&format!("`{}` must be defined only once in the {} namespace of this {}",
                          name,
                          namespace,
                          container));

        err.span_label(span, format!("`{}` re{} here", name, new_participle));
        if old_binding.span != syntax_pos::DUMMY_SP {
            err.span_label(old_binding.span, format!("previous {} of the {} `{}` here",
                                                      old_noun, old_kind, name));
        }

        err.emit();
        self.name_already_seen.insert(name, span);
    }

    fn warn_legacy_self_import(&self, directive: &'a ImportDirective<'a>) {
        let (id, span) = (directive.id, directive.span);
        let msg = "`self` no longer imports values".to_string();
        self.session.add_lint(lint::builtin::LEGACY_IMPORTS, id, span, msg);
    }

    fn check_proc_macro_attrs(&mut self, attrs: &[ast::Attribute]) {
        if self.proc_macro_enabled { return; }

        for attr in attrs {
            if attr.path.segments.len() > 1 {
                continue
            }
            let ident = attr.path.segments[0].identifier;
            let result = self.resolve_lexical_macro_path_segment(ident,
                                                                 MacroNS,
                                                                 false,
                                                                 attr.path.span);
            if let Ok(binding) = result {
                if let SyntaxExtension::AttrProcMacro(..) = *binding.binding().get_macro(self) {
                    attr::mark_known(attr);

                    let msg = "attribute procedural macros are experimental";
                    let feature = "proc_macro";

                    feature_err(&self.session.parse_sess, feature,
                                attr.span, GateIssue::Language, msg)
                        .span_note(binding.span(), "procedural macro imported here")
                        .emit();
                }
            }
        }
    }
}

fn is_struct_like(def: Def) -> bool {
    match def {
        Def::VariantCtor(_, CtorKind::Fictive) => true,
        _ => PathSource::Struct.is_expected(def),
    }
}

fn is_self_type(path: &[SpannedIdent], namespace: Namespace) -> bool {
    namespace == TypeNS && path.len() == 1 && path[0].node.name == keywords::SelfType.name()
}

fn is_self_value(path: &[SpannedIdent], namespace: Namespace) -> bool {
    namespace == ValueNS && path.len() == 1 && path[0].node.name == keywords::SelfValue.name()
}

fn names_to_string(idents: &[SpannedIdent]) -> String {
    let mut result = String::new();
    for (i, ident) in idents.iter()
                            .filter(|i| i.node.name != keywords::CrateRoot.name())
                            .enumerate() {
        if i > 0 {
            result.push_str("::");
        }
        result.push_str(&ident.node.name.as_str());
    }
    result
}

fn path_names_to_string(path: &Path) -> String {
    names_to_string(&path.segments.iter()
                        .map(|seg| respan(seg.span, seg.identifier))
                        .collect::<Vec<_>>())
}

/// Get the path for an enum and the variant from an `ImportSuggestion` for an enum variant.
fn import_candidate_to_paths(suggestion: &ImportSuggestion) -> (Span, String, String) {
    let variant_path = &suggestion.path;
    let variant_path_string = path_names_to_string(variant_path);

    let path_len = suggestion.path.segments.len();
    let enum_path = ast::Path {
        span: suggestion.path.span,
        segments: suggestion.path.segments[0..path_len - 1].to_vec(),
    };
    let enum_path_string = path_names_to_string(&enum_path);

    (suggestion.path.span, variant_path_string, enum_path_string)
}


/// When an entity with a given name is not available in scope, we search for
/// entities with that name in all crates. This method allows outputting the
/// results of this search in a programmer-friendly way
fn show_candidates(err: &mut DiagnosticBuilder,
                   span: Span,
                   candidates: &[ImportSuggestion],
                   better: bool) {

    // we want consistent results across executions, but candidates are produced
    // by iterating through a hash map, so make sure they are ordered:
    let mut path_strings: Vec<_> =
        candidates.into_iter().map(|c| path_names_to_string(&c.path)).collect();
    path_strings.sort();

    let better = if better { "better " } else { "" };
    let msg_diff = match path_strings.len() {
        1 => " is found in another module, you can import it",
        _ => "s are found in other modules, you can import them",
    };
    let msg = format!("possible {}candidate{} into scope", better, msg_diff);

    for candidate in &mut path_strings {
        *candidate = format!("use {};\n", candidate);
    }

    err.span_suggestions(span, &msg, path_strings);
}

/// A somewhat inefficient routine to obtain the name of a module.
fn module_to_string(module: Module) -> String {
    let mut names = Vec::new();

    fn collect_mod(names: &mut Vec<Ident>, module: Module) {
        if let ModuleKind::Def(_, name) = module.kind {
            if let Some(parent) = module.parent {
                names.push(Ident::with_empty_ctxt(name));
                collect_mod(names, parent);
            }
        } else {
            // danger, shouldn't be ident?
            names.push(Ident::from_str("<opaque>"));
            collect_mod(names, module.parent.unwrap());
        }
    }
    collect_mod(&mut names, module);

    if names.is_empty() {
        return "???".to_string();
    }
    names_to_string(&names.into_iter()
                        .rev()
                        .map(|n| dummy_spanned(n))
                        .collect::<Vec<_>>())
}

fn err_path_resolution() -> PathResolution {
    PathResolution::new(Def::Err)
}

#[derive(PartialEq,Copy, Clone)]
pub enum MakeGlobMap {
    Yes,
    No,
}

__build_diagnostic_array! { librustc_resolve, DIAGNOSTICS }
