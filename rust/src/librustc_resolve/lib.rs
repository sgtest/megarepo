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
#![unstable(feature = "rustc_private", issue = "27812")]
#![crate_type = "dylib"]
#![crate_type = "rlib"]
#![doc(html_logo_url = "https://www.rust-lang.org/logos/rust-logo-128x128-blk-v2.png",
      html_favicon_url = "https://doc.rust-lang.org/favicon.ico",
      html_root_url = "https://doc.rust-lang.org/nightly/")]
#![cfg_attr(not(stage0), deny(warnings))]

#![feature(associated_consts)]
#![feature(borrow_state)]
#![feature(rustc_diagnostic_macros)]
#![feature(rustc_private)]
#![feature(staged_api)]

#[macro_use]
extern crate log;
#[macro_use]
extern crate syntax;
extern crate arena;
#[macro_use]
extern crate rustc;

use self::PatternBindingMode::*;
use self::Namespace::*;
use self::ResolveResult::*;
use self::FallbackSuggestion::*;
use self::TypeParameters::*;
use self::RibKind::*;
use self::UseLexicalScopeFlag::*;
use self::ModulePrefixResult::*;
use self::AssocItemResolveResult::*;
use self::BareIdentifierPatternResolution::*;
use self::ParentLink::*;
use self::FallbackChecks::*;

use rustc::dep_graph::DepNode;
use rustc::hir::map as hir_map;
use rustc::session::Session;
use rustc::lint;
use rustc::middle::cstore::CrateStore;
use rustc::hir::def::*;
use rustc::hir::def_id::DefId;
use rustc::hir::pat_util::pat_bindings;
use rustc::ty;
use rustc::ty::subst::{ParamSpace, FnSpace, TypeSpace};
use rustc::hir::{Freevar, FreevarMap, TraitMap, GlobMap};
use rustc::util::nodemap::{NodeMap, FnvHashMap, FnvHashSet};

use syntax::ast::{self, FloatTy};
use syntax::ast::{CRATE_NODE_ID, Name, NodeId, CrateNum, IntTy, UintTy};
use syntax::attr::AttrMetaMethods;
use syntax::codemap::{self, Span, Pos};
use syntax::errors::DiagnosticBuilder;
use syntax::parse::token::{self, special_names, special_idents};
use syntax::util::lev_distance::find_best_match_for_name;

use rustc::hir::intravisit::{self, FnKind, Visitor};
use rustc::hir;
use rustc::hir::{Arm, BindByRef, BindByValue, BindingMode, Block};
use rustc::hir::Crate;
use rustc::hir::{Expr, ExprAgain, ExprBreak, ExprCall, ExprField};
use rustc::hir::{ExprLoop, ExprWhile, ExprMethodCall};
use rustc::hir::{ExprPath, ExprStruct, FnDecl};
use rustc::hir::{ForeignItemFn, ForeignItemStatic, Generics};
use rustc::hir::{ImplItem, Item, ItemConst, ItemEnum, ItemExternCrate};
use rustc::hir::{ItemFn, ItemForeignMod, ItemImpl, ItemMod, ItemStatic, ItemDefaultImpl};
use rustc::hir::{ItemStruct, ItemTrait, ItemTy, ItemUse};
use rustc::hir::Local;
use rustc::hir::{Pat, PatKind, Path, PrimTy};
use rustc::hir::{PathSegment, PathParameters};
use rustc::hir::HirVec;
use rustc::hir::{TraitRef, Ty, TyBool, TyChar, TyFloat, TyInt};
use rustc::hir::{TyRptr, TyStr, TyUint, TyPath, TyPtr};

use std::collections::{HashMap, HashSet};
use std::cell::{Cell, RefCell};
use std::fmt;
use std::mem::replace;

use resolve_imports::{ImportDirective, NameResolution};

// NB: This module needs to be declared first so diagnostics are
// registered before they are used.
mod diagnostics;

mod check_unused;
mod build_reduced_graph;
mod resolve_imports;

// Perform the callback, not walking deeper if the return is true
macro_rules! execute_callback {
    ($node: expr, $walker: expr) => (
        if let Some(ref callback) = $walker.callback {
            if callback($node, &mut $walker.resolved) {
                return;
            }
        }
    )
}

enum SuggestionType {
    Macro(String),
    Function(token::InternedString),
    NotFound,
}

/// Candidates for a name resolution failure
struct SuggestedCandidates {
    name: String,
    candidates: Vec<Path>,
}

enum ResolutionError<'a> {
    /// error E0401: can't use type parameters from outer function
    TypeParametersFromOuterFunction,
    /// error E0402: cannot use an outer type parameter in this context
    OuterTypeParameterContext,
    /// error E0403: the name is already used for a type parameter in this type parameter list
    NameAlreadyUsedInTypeParameterList(Name),
    /// error E0404: is not a trait
    IsNotATrait(&'a str),
    /// error E0405: use of undeclared trait name
    UndeclaredTraitName(&'a str, SuggestedCandidates),
    /// error E0406: undeclared associated type
    UndeclaredAssociatedType,
    /// error E0407: method is not a member of trait
    MethodNotMemberOfTrait(Name, &'a str),
    /// error E0437: type is not a member of trait
    TypeNotMemberOfTrait(Name, &'a str),
    /// error E0438: const is not a member of trait
    ConstNotMemberOfTrait(Name, &'a str),
    /// error E0408: variable `{}` from pattern #1 is not bound in pattern
    VariableNotBoundInPattern(Name, usize),
    /// error E0409: variable is bound with different mode in pattern #{} than in pattern #1
    VariableBoundWithDifferentMode(Name, usize),
    /// error E0410: variable from pattern is not bound in pattern #1
    VariableNotBoundInParentPattern(Name, usize),
    /// error E0411: use of `Self` outside of an impl or trait
    SelfUsedOutsideImplOrTrait,
    /// error E0412: use of undeclared
    UseOfUndeclared(&'a str, &'a str, SuggestedCandidates),
    /// error E0413: declaration shadows an enum variant or unit-like struct in scope
    DeclarationShadowsEnumVariantOrUnitLikeStruct(Name),
    /// error E0414: only irrefutable patterns allowed here
    OnlyIrrefutablePatternsAllowedHere(DefId, Name),
    /// error E0415: identifier is bound more than once in this parameter list
    IdentifierBoundMoreThanOnceInParameterList(&'a str),
    /// error E0416: identifier is bound more than once in the same pattern
    IdentifierBoundMoreThanOnceInSamePattern(&'a str),
    /// error E0417: static variables cannot be referenced in a pattern
    StaticVariableReference,
    /// error E0418: is not an enum variant, struct or const
    NotAnEnumVariantStructOrConst(&'a str),
    /// error E0419: unresolved enum variant, struct or const
    UnresolvedEnumVariantStructOrConst(&'a str),
    /// error E0420: is not an associated const
    NotAnAssociatedConst(&'a str),
    /// error E0421: unresolved associated const
    UnresolvedAssociatedConst(&'a str),
    /// error E0422: does not name a struct
    DoesNotNameAStruct(&'a str),
    /// error E0423: is a struct variant name, but this expression uses it like a function name
    StructVariantUsedAsFunction(&'a str),
    /// error E0424: `self` is not available in a static method
    SelfNotAvailableInStaticMethod,
    /// error E0425: unresolved name
    UnresolvedName(&'a str, &'a str, UnresolvedNameContext),
    /// error E0426: use of undeclared label
    UndeclaredLabel(&'a str),
    /// error E0427: cannot use `ref` binding mode with ...
    CannotUseRefBindingModeWith(&'a str),
    /// error E0429: `self` imports are only allowed within a { } list
    SelfImportsOnlyAllowedWithin,
    /// error E0430: `self` import can only appear once in the list
    SelfImportCanOnlyAppearOnceInTheList,
    /// error E0431: `self` import can only appear in an import list with a non-empty prefix
    SelfImportOnlyInImportListWithNonEmptyPrefix,
    /// error E0432: unresolved import
    UnresolvedImport(Option<(&'a str, &'a str)>),
    /// error E0433: failed to resolve
    FailedToResolve(&'a str),
    /// error E0434: can't capture dynamic environment in a fn item
    CannotCaptureDynamicEnvironmentInFnItem,
    /// error E0435: attempt to use a non-constant value in a constant
    AttemptToUseNonConstantValueInConstant,
}

/// Context of where `ResolutionError::UnresolvedName` arose.
#[derive(Clone, PartialEq, Eq, Debug)]
enum UnresolvedNameContext {
    /// `PathIsMod(id)` indicates that a given path, used in
    /// expression context, actually resolved to a module rather than
    /// a value. The `id` attached to the variant is the node id of
    /// the erroneous path expression.
    PathIsMod(ast::NodeId),

    /// `Other` means we have no extra information about the context
    /// of the unresolved name error. (Maybe we could eliminate all
    /// such cases; but for now, this is an information-free default.)
    Other,
}

fn resolve_error<'b, 'a: 'b, 'tcx: 'a>(resolver: &'b Resolver<'a, 'tcx>,
                                       span: syntax::codemap::Span,
                                       resolution_error: ResolutionError<'b>) {
    resolve_struct_error(resolver, span, resolution_error).emit();
}

fn resolve_struct_error<'b, 'a: 'b, 'tcx: 'a>(resolver: &'b Resolver<'a, 'tcx>,
                                              span: syntax::codemap::Span,
                                              resolution_error: ResolutionError<'b>)
                                              -> DiagnosticBuilder<'a> {
    if !resolver.emit_errors {
        return resolver.session.diagnostic().struct_dummy();
    }

    match resolution_error {
        ResolutionError::TypeParametersFromOuterFunction => {
            struct_span_err!(resolver.session,
                             span,
                             E0401,
                             "can't use type parameters from outer function; try using a local \
                              type parameter instead")
        }
        ResolutionError::OuterTypeParameterContext => {
            struct_span_err!(resolver.session,
                             span,
                             E0402,
                             "cannot use an outer type parameter in this context")
        }
        ResolutionError::NameAlreadyUsedInTypeParameterList(name) => {
            struct_span_err!(resolver.session,
                             span,
                             E0403,
                             "the name `{}` is already used for a type parameter in this type \
                              parameter list",
                             name)
        }
        ResolutionError::IsNotATrait(name) => {
            struct_span_err!(resolver.session, span, E0404, "`{}` is not a trait", name)
        }
        ResolutionError::UndeclaredTraitName(name, candidates) => {
            let mut err = struct_span_err!(resolver.session,
                                           span,
                                           E0405,
                                           "trait `{}` is not in scope",
                                           name);
            show_candidates(&mut err, span, &candidates);
            err
        }
        ResolutionError::UndeclaredAssociatedType => {
            struct_span_err!(resolver.session, span, E0406, "undeclared associated type")
        }
        ResolutionError::MethodNotMemberOfTrait(method, trait_) => {
            struct_span_err!(resolver.session,
                             span,
                             E0407,
                             "method `{}` is not a member of trait `{}`",
                             method,
                             trait_)
        }
        ResolutionError::TypeNotMemberOfTrait(type_, trait_) => {
            struct_span_err!(resolver.session,
                             span,
                             E0437,
                             "type `{}` is not a member of trait `{}`",
                             type_,
                             trait_)
        }
        ResolutionError::ConstNotMemberOfTrait(const_, trait_) => {
            struct_span_err!(resolver.session,
                             span,
                             E0438,
                             "const `{}` is not a member of trait `{}`",
                             const_,
                             trait_)
        }
        ResolutionError::VariableNotBoundInPattern(variable_name, pattern_number) => {
            struct_span_err!(resolver.session,
                             span,
                             E0408,
                             "variable `{}` from pattern #1 is not bound in pattern #{}",
                             variable_name,
                             pattern_number)
        }
        ResolutionError::VariableBoundWithDifferentMode(variable_name, pattern_number) => {
            struct_span_err!(resolver.session,
                             span,
                             E0409,
                             "variable `{}` is bound with different mode in pattern #{} than in \
                              pattern #1",
                             variable_name,
                             pattern_number)
        }
        ResolutionError::VariableNotBoundInParentPattern(variable_name, pattern_number) => {
            struct_span_err!(resolver.session,
                             span,
                             E0410,
                             "variable `{}` from pattern #{} is not bound in pattern #1",
                             variable_name,
                             pattern_number)
        }
        ResolutionError::SelfUsedOutsideImplOrTrait => {
            struct_span_err!(resolver.session,
                             span,
                             E0411,
                             "use of `Self` outside of an impl or trait")
        }
        ResolutionError::UseOfUndeclared(kind, name, candidates) => {
            let mut err = struct_span_err!(resolver.session,
                                           span,
                                           E0412,
                                           "{} `{}` is undefined or not in scope",
                                           kind,
                                           name);
            show_candidates(&mut err, span, &candidates);
            err
        }
        ResolutionError::DeclarationShadowsEnumVariantOrUnitLikeStruct(name) => {
            struct_span_err!(resolver.session,
                             span,
                             E0413,
                             "declaration of `{}` shadows an enum variant \
                              or unit-like struct in scope",
                             name)
        }
        ResolutionError::OnlyIrrefutablePatternsAllowedHere(did, name) => {
            let mut err = struct_span_err!(resolver.session,
                                           span,
                                           E0414,
                                           "only irrefutable patterns allowed here");
            err.span_note(span,
                          "there already is a constant in scope sharing the same \
                           name as this pattern");
            if let Some(sp) = resolver.ast_map.span_if_local(did) {
                err.span_note(sp, "constant defined here");
            }
            if let Some(binding) = resolver.current_module
                                           .resolve_name_in_lexical_scope(name, ValueNS) {
                if binding.is_import() {
                    err.span_note(binding.span.unwrap(), "constant imported here");
                }
            }
            err
        }
        ResolutionError::IdentifierBoundMoreThanOnceInParameterList(identifier) => {
            struct_span_err!(resolver.session,
                             span,
                             E0415,
                             "identifier `{}` is bound more than once in this parameter list",
                             identifier)
        }
        ResolutionError::IdentifierBoundMoreThanOnceInSamePattern(identifier) => {
            struct_span_err!(resolver.session,
                             span,
                             E0416,
                             "identifier `{}` is bound more than once in the same pattern",
                             identifier)
        }
        ResolutionError::StaticVariableReference => {
            struct_span_err!(resolver.session,
                             span,
                             E0417,
                             "static variables cannot be referenced in a pattern, use a \
                              `const` instead")
        }
        ResolutionError::NotAnEnumVariantStructOrConst(name) => {
            struct_span_err!(resolver.session,
                             span,
                             E0418,
                             "`{}` is not an enum variant, struct or const",
                             name)
        }
        ResolutionError::UnresolvedEnumVariantStructOrConst(name) => {
            struct_span_err!(resolver.session,
                             span,
                             E0419,
                             "unresolved enum variant, struct or const `{}`",
                             name)
        }
        ResolutionError::NotAnAssociatedConst(name) => {
            struct_span_err!(resolver.session,
                             span,
                             E0420,
                             "`{}` is not an associated const",
                             name)
        }
        ResolutionError::UnresolvedAssociatedConst(name) => {
            struct_span_err!(resolver.session,
                             span,
                             E0421,
                             "unresolved associated const `{}`",
                             name)
        }
        ResolutionError::DoesNotNameAStruct(name) => {
            struct_span_err!(resolver.session,
                             span,
                             E0422,
                             "`{}` does not name a structure",
                             name)
        }
        ResolutionError::StructVariantUsedAsFunction(path_name) => {
            struct_span_err!(resolver.session,
                             span,
                             E0423,
                             "`{}` is the name of a struct or struct variant, but this expression \
                             uses it like a function name",
                             path_name)
        }
        ResolutionError::SelfNotAvailableInStaticMethod => {
            struct_span_err!(resolver.session,
                             span,
                             E0424,
                             "`self` is not available in a static method. Maybe a `self` \
                             argument is missing?")
        }
        ResolutionError::UnresolvedName(path, msg, context) => {
            let mut err = struct_span_err!(resolver.session,
                                           span,
                                           E0425,
                                           "unresolved name `{}`{}",
                                           path,
                                           msg);

            match context {
                UnresolvedNameContext::Other => { } // no help available
                UnresolvedNameContext::PathIsMod(id) => {
                    let mut help_msg = String::new();
                    let parent_id = resolver.ast_map.get_parent_node(id);
                    if let Some(hir_map::Node::NodeExpr(e)) = resolver.ast_map.find(parent_id) {
                        match e.node {
                            ExprField(_, ident) => {
                                help_msg = format!("To reference an item from the \
                                                    `{module}` module, use \
                                                    `{module}::{ident}`",
                                                   module = path,
                                                   ident = ident.node);
                            }
                            ExprMethodCall(ident, _, _) => {
                                help_msg = format!("To call a function from the \
                                                    `{module}` module, use \
                                                    `{module}::{ident}(..)`",
                                                   module = path,
                                                   ident = ident.node);
                            }
                            ExprCall(_, _) => {
                                help_msg = format!("No function corresponds to `{module}(..)`",
                                                   module = path);
                            }
                            _ => { } // no help available
                        }
                    } else {
                        help_msg = format!("Module `{module}` cannot be the value of an expression",
                                           module = path);
                    }

                    if !help_msg.is_empty() {
                        err.fileline_help(span, &help_msg);
                    }
                }
            }
            err
        }
        ResolutionError::UndeclaredLabel(name) => {
            struct_span_err!(resolver.session,
                             span,
                             E0426,
                             "use of undeclared label `{}`",
                             name)
        }
        ResolutionError::CannotUseRefBindingModeWith(descr) => {
            struct_span_err!(resolver.session,
                             span,
                             E0427,
                             "cannot use `ref` binding mode with {}",
                             descr)
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
            let msg = match name {
                Some((n, p)) => format!("unresolved import `{}`{}", n, p),
                None => "unresolved import".to_owned(),
            };
            struct_span_err!(resolver.session, span, E0432, "{}", msg)
        }
        ResolutionError::FailedToResolve(msg) => {
            struct_span_err!(resolver.session, span, E0433, "failed to resolve. {}", msg)
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
            struct_span_err!(resolver.session,
                             span,
                             E0435,
                             "attempt to use a non-constant value in a constant")
        }
    }
}

#[derive(Copy, Clone)]
struct BindingInfo {
    span: Span,
    binding_mode: BindingMode,
}

// Map from the name in a pattern to its binding mode.
type BindingMap = HashMap<Name, BindingInfo>;

#[derive(Copy, Clone, PartialEq)]
enum PatternBindingMode {
    RefutableMode,
    LocalIrrefutableMode,
    ArgumentIrrefutableMode,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Namespace {
    TypeNS,
    ValueNS,
}

impl<'a, 'v, 'tcx> Visitor<'v> for Resolver<'a, 'tcx> {
    fn visit_nested_item(&mut self, item: hir::ItemId) {
        self.visit_item(self.ast_map.expect_item(item.id))
    }
    fn visit_item(&mut self, item: &Item) {
        execute_callback!(hir_map::Node::NodeItem(item), self);
        self.resolve_item(item);
    }
    fn visit_arm(&mut self, arm: &Arm) {
        self.resolve_arm(arm);
    }
    fn visit_block(&mut self, block: &Block) {
        execute_callback!(hir_map::Node::NodeBlock(block), self);
        self.resolve_block(block);
    }
    fn visit_expr(&mut self, expr: &Expr) {
        execute_callback!(hir_map::Node::NodeExpr(expr), self);
        self.resolve_expr(expr);
    }
    fn visit_local(&mut self, local: &Local) {
        execute_callback!(hir_map::Node::NodeLocal(&local.pat), self);
        self.resolve_local(local);
    }
    fn visit_ty(&mut self, ty: &Ty) {
        self.resolve_type(ty);
    }
    fn visit_generics(&mut self, generics: &Generics) {
        self.resolve_generics(generics);
    }
    fn visit_poly_trait_ref(&mut self, tref: &hir::PolyTraitRef, m: &hir::TraitBoundModifier) {
        match self.resolve_trait_reference(tref.trait_ref.ref_id, &tref.trait_ref.path, 0) {
            Ok(def) => self.record_def(tref.trait_ref.ref_id, def),
            Err(_) => {
                // error already reported
                self.record_def(tref.trait_ref.ref_id, err_path_resolution())
            }
        }
        intravisit::walk_poly_trait_ref(self, tref, m);
    }
    fn visit_variant(&mut self,
                     variant: &hir::Variant,
                     generics: &Generics,
                     item_id: ast::NodeId) {
        execute_callback!(hir_map::Node::NodeVariant(variant), self);
        if let Some(ref dis_expr) = variant.node.disr_expr {
            // resolve the discriminator expr as a constant
            self.with_constant_rib(|this| {
                this.visit_expr(dis_expr);
            });
        }

        // `intravisit::walk_variant` without the discriminant expression.
        self.visit_variant_data(&variant.node.data,
                                variant.node.name,
                                generics,
                                item_id,
                                variant.span);
    }
    fn visit_foreign_item(&mut self, foreign_item: &hir::ForeignItem) {
        execute_callback!(hir_map::Node::NodeForeignItem(foreign_item), self);
        let type_parameters = match foreign_item.node {
            ForeignItemFn(_, ref generics) => {
                HasTypeParameters(generics, FnSpace, ItemRibKind)
            }
            ForeignItemStatic(..) => NoTypeParameters,
        };
        self.with_type_parameter_rib(type_parameters, |this| {
            intravisit::walk_foreign_item(this, foreign_item);
        });
    }
    fn visit_fn(&mut self,
                function_kind: FnKind<'v>,
                declaration: &'v FnDecl,
                block: &'v Block,
                _: Span,
                node_id: NodeId) {
        let rib_kind = match function_kind {
            FnKind::ItemFn(_, generics, _, _, _, _, _) => {
                self.visit_generics(generics);
                ItemRibKind
            }
            FnKind::Method(_, sig, _, _) => {
                self.visit_generics(&sig.generics);
                self.visit_explicit_self(&sig.explicit_self);
                MethodRibKind
            }
            FnKind::Closure(_) => ClosureRibKind(node_id),
        };
        self.resolve_function(rib_kind, declaration, block);
    }
}

pub type ErrorMessage = Option<(Span, String)>;

#[derive(Clone, PartialEq, Eq)]
pub enum ResolveResult<T> {
    Failed(ErrorMessage), // Failed to resolve the name, optional helpful error message.
    Indeterminate, // Couldn't determine due to unresolved globs.
    Success(T), // Successfully resolved the import.
}

impl<T> ResolveResult<T> {
    fn and_then<U, F: FnOnce(T) -> ResolveResult<U>>(self, f: F) -> ResolveResult<U> {
        match self {
            Failed(msg) => Failed(msg),
            Indeterminate => Indeterminate,
            Success(t) => f(t),
        }
    }

    fn success(self) -> Option<T> {
        match self {
            Success(t) => Some(t),
            _ => None,
        }
    }
}

enum FallbackSuggestion {
    NoSuggestion,
    Field,
    Method,
    TraitItem,
    StaticMethod(String),
    TraitMethod(String),
}

#[derive(Copy, Clone)]
enum TypeParameters<'tcx, 'a> {
    NoTypeParameters,
    HasTypeParameters(// Type parameters.
                      &'a Generics,

                      // Identifies the things that these parameters
                      // were declared on (type, fn, etc)
                      ParamSpace,

                      // The kind of the rib used for type parameters.
                      RibKind<'tcx>),
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
    MethodRibKind,

    // We passed through an item scope. Disallow upvars.
    ItemRibKind,

    // We're in a constant item. Can't refer to dynamic stuff.
    ConstantItemRibKind,

    // We passed through a module.
    ModuleRibKind(Module<'a>),
}

#[derive(Copy, Clone)]
enum UseLexicalScopeFlag {
    DontUseLexicalScope,
    UseLexicalScope,
}

enum ModulePrefixResult<'a> {
    NoPrefixFound,
    PrefixFound(Module<'a>, usize),
}

#[derive(Copy, Clone)]
enum AssocItemResolveResult {
    /// Syntax such as `<T>::item`, which can't be resolved until type
    /// checking.
    TypecheckRequired,
    /// We should have been able to resolve the associated item.
    ResolveAttempt(Option<PathResolution>),
}

#[derive(Copy, Clone)]
enum BareIdentifierPatternResolution {
    FoundStructOrEnumVariant(Def),
    FoundConst(Def, Name),
    BareIdentifierPatternUnresolved,
}

/// One local scope.
#[derive(Debug)]
struct Rib<'a> {
    bindings: HashMap<Name, Def>,
    kind: RibKind<'a>,
}

impl<'a> Rib<'a> {
    fn new(kind: RibKind<'a>) -> Rib<'a> {
        Rib {
            bindings: HashMap::new(),
            kind: kind,
        }
    }
}

/// A definition along with the index of the rib it was found on
struct LocalDef {
    ribs: Option<(Namespace, usize)>,
    def: Def,
}

impl LocalDef {
    fn from_def(def: Def) -> Self {
        LocalDef {
            ribs: None,
            def: def,
        }
    }
}

enum LexicalScopeBinding<'a> {
    Item(&'a NameBinding<'a>),
    LocalDef(LocalDef),
}

impl<'a> LexicalScopeBinding<'a> {
    fn local_def(self) -> LocalDef {
        match self {
            LexicalScopeBinding::LocalDef(local_def) => local_def,
            LexicalScopeBinding::Item(binding) => LocalDef::from_def(binding.def().unwrap()),
        }
    }

    fn def(self) -> Def {
        self.local_def().def
    }

    fn module(self) -> Option<Module<'a>> {
        match self {
            LexicalScopeBinding::Item(binding) => binding.module(),
            _ => None,
        }
    }
}

/// The link from a module up to its nearest parent node.
#[derive(Clone,Debug)]
enum ParentLink<'a> {
    NoParentLink,
    ModuleParentLink(Module<'a>, Name),
    BlockParentLink(Module<'a>, NodeId),
}

/// One node in the tree of modules.
pub struct ModuleS<'a> {
    parent_link: ParentLink<'a>,
    def: Option<Def>,
    vis: ty::Visibility,

    // If the module is an extern crate, `def` is root of the external crate and `extern_crate_id`
    // is the NodeId of the local `extern crate` item (otherwise, `extern_crate_id` is None).
    extern_crate_id: Option<NodeId>,

    resolutions: RefCell<HashMap<(Name, Namespace), &'a RefCell<NameResolution<'a>>>>,
    unresolved_imports: RefCell<Vec<&'a ImportDirective<'a>>>,

    // The module children of this node, including normal modules and anonymous modules.
    // Anonymous children are pseudo-modules that are implicitly created around items
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
    module_children: RefCell<NodeMap<Module<'a>>>,

    prelude: RefCell<Option<Module<'a>>>,

    glob_importers: RefCell<Vec<(Module<'a>, &'a ImportDirective<'a>)>>,
    globs: RefCell<Vec<&'a ImportDirective<'a>>>,

    // Whether this module is populated. If not populated, any attempt to
    // access the children must be preceded with a
    // `populate_module_if_necessary` call.
    populated: Cell<bool>,

    arenas: &'a ResolverArenas<'a>,
}

pub type Module<'a> = &'a ModuleS<'a>;

impl<'a> ModuleS<'a> {
    fn new(parent_link: ParentLink<'a>,
           def: Option<Def>,
           external: bool,
           vis: ty::Visibility,
           arenas: &'a ResolverArenas<'a>) -> Self {
        ModuleS {
            parent_link: parent_link,
            def: def,
            vis: vis,
            extern_crate_id: None,
            resolutions: RefCell::new(HashMap::new()),
            unresolved_imports: RefCell::new(Vec::new()),
            module_children: RefCell::new(NodeMap()),
            prelude: RefCell::new(None),
            glob_importers: RefCell::new(Vec::new()),
            globs: RefCell::new((Vec::new())),
            populated: Cell::new(!external),
            arenas: arenas
        }
    }

    fn for_each_child<F: FnMut(Name, Namespace, &'a NameBinding<'a>)>(&self, mut f: F) {
        for (&(name, ns), name_resolution) in self.resolutions.borrow().iter() {
            name_resolution.borrow().binding.map(|binding| f(name, ns, binding));
        }
    }

    fn def_id(&self) -> Option<DefId> {
        self.def.as_ref().map(Def::def_id)
    }

    // `self` resolves to the first module ancestor that `is_normal`.
    fn is_normal(&self) -> bool {
        match self.def {
            Some(Def::Mod(_)) => true,
            _ => false,
        }
    }

    fn is_trait(&self) -> bool {
        match self.def {
            Some(Def::Trait(_)) => true,
            _ => false,
        }
    }
}

impl<'a> fmt::Debug for ModuleS<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}, {:?}", self.def, self.vis)
    }
}

// Records a possibly-private value, type, or module definition.
#[derive(Clone, Debug)]
pub struct NameBinding<'a> {
    kind: NameBindingKind<'a>,
    span: Option<Span>,
    vis: ty::Visibility,
}

#[derive(Clone, Debug)]
enum NameBindingKind<'a> {
    Def(Def),
    Module(Module<'a>),
    Import {
        binding: &'a NameBinding<'a>,
        directive: &'a ImportDirective<'a>,
        // Some(error) if using this imported name causes the import to be a privacy error
        privacy_error: Option<Box<PrivacyError<'a>>>,
    },
}

#[derive(Clone, Debug)]
struct PrivacyError<'a>(Span, Name, &'a NameBinding<'a>);

impl<'a> NameBinding<'a> {
    fn create_from_module(module: Module<'a>, span: Option<Span>) -> Self {
        NameBinding {
            kind: NameBindingKind::Module(module),
            span: span,
            vis: module.vis,
        }
    }

    fn module(&self) -> Option<Module<'a>> {
        match self.kind {
            NameBindingKind::Module(module) => Some(module),
            NameBindingKind::Def(_) => None,
            NameBindingKind::Import { binding, .. } => binding.module(),
        }
    }

    fn def(&self) -> Option<Def> {
        match self.kind {
            NameBindingKind::Def(def) => Some(def),
            NameBindingKind::Module(module) => module.def,
            NameBindingKind::Import { binding, .. } => binding.def(),
        }
    }

    fn is_pseudo_public(&self) -> bool {
        self.pseudo_vis() == ty::Visibility::Public
    }

    // We sometimes need to treat variants as `pub` for backwards compatibility
    fn pseudo_vis(&self) -> ty::Visibility {
        if self.is_variant() { ty::Visibility::Public } else { self.vis }
    }

    fn is_variant(&self) -> bool {
        match self.kind {
            NameBindingKind::Def(Def::Variant(..)) => true,
            _ => false,
        }
    }

    fn is_extern_crate(&self) -> bool {
        self.module().and_then(|module| module.extern_crate_id).is_some()
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
            _ => false,
        }
    }

    fn is_importable(&self) -> bool {
        match self.def().unwrap() {
            Def::AssociatedConst(..) | Def::Method(..) | Def::AssociatedTy(..) => false,
            _ => true,
        }
    }
}

/// Interns the names of the primitive types.
struct PrimitiveTypeTable {
    primitive_types: HashMap<Name, PrimTy>,
}

impl PrimitiveTypeTable {
    fn new() -> PrimitiveTypeTable {
        let mut table = PrimitiveTypeTable { primitive_types: HashMap::new() };

        table.intern("bool", TyBool);
        table.intern("char", TyChar);
        table.intern("f32", TyFloat(FloatTy::F32));
        table.intern("f64", TyFloat(FloatTy::F64));
        table.intern("isize", TyInt(IntTy::Is));
        table.intern("i8", TyInt(IntTy::I8));
        table.intern("i16", TyInt(IntTy::I16));
        table.intern("i32", TyInt(IntTy::I32));
        table.intern("i64", TyInt(IntTy::I64));
        table.intern("str", TyStr);
        table.intern("usize", TyUint(UintTy::Us));
        table.intern("u8", TyUint(UintTy::U8));
        table.intern("u16", TyUint(UintTy::U16));
        table.intern("u32", TyUint(UintTy::U32));
        table.intern("u64", TyUint(UintTy::U64));

        table
    }

    fn intern(&mut self, string: &str, primitive_type: PrimTy) {
        self.primitive_types.insert(token::intern(string), primitive_type);
    }
}

/// The main resolver class.
pub struct Resolver<'a, 'tcx: 'a> {
    session: &'a Session,

    ast_map: &'a hir_map::Map<'tcx>,

    graph_root: Module<'a>,

    trait_item_map: FnvHashMap<(Name, DefId), DefId>,

    structs: FnvHashMap<DefId, Vec<Name>>,

    // The number of imports that are currently unresolved.
    unresolved_imports: usize,

    // The module that represents the current item scope.
    current_module: Module<'a>,

    // The current set of local scopes, for values.
    // FIXME #4948: Reuse ribs to avoid allocation.
    value_ribs: Vec<Rib<'a>>,

    // The current set of local scopes, for types.
    type_ribs: Vec<Rib<'a>>,

    // The current set of local scopes, for labels.
    label_ribs: Vec<Rib<'a>>,

    // The trait that the current context can refer to.
    current_trait_ref: Option<(DefId, TraitRef)>,

    // The current self type if inside an impl (used for better errors).
    current_self_type: Option<Ty>,

    // The idents for the primitive types.
    primitive_type_table: PrimitiveTypeTable,

    def_map: RefCell<DefMap>,
    freevars: FreevarMap,
    freevars_seen: NodeMap<NodeMap<usize>>,
    export_map: ExportMap,
    trait_map: TraitMap,

    // Whether or not to print error messages. Can be set to true
    // when getting additional info for error message suggestions,
    // so as to avoid printing duplicate errors
    emit_errors: bool,

    make_glob_map: bool,
    // Maps imports to the names of items actually imported (this actually maps
    // all imports, but only glob imports are actually interesting).
    glob_map: GlobMap,

    used_imports: HashSet<(NodeId, Namespace)>,
    used_crates: HashSet<CrateNum>,

    // Callback function for intercepting walks
    callback: Option<Box<Fn(hir_map::Node, &mut bool) -> bool>>,
    // The intention is that the callback modifies this flag.
    // Once set, the resolver falls out of the walk, preserving the ribs.
    resolved: bool,
    privacy_errors: Vec<PrivacyError<'a>>,

    arenas: &'a ResolverArenas<'a>,
}

struct ResolverArenas<'a> {
    modules: arena::TypedArena<ModuleS<'a>>,
    name_bindings: arena::TypedArena<NameBinding<'a>>,
    import_directives: arena::TypedArena<ImportDirective<'a>>,
    name_resolutions: arena::TypedArena<RefCell<NameResolution<'a>>>,
}

impl<'a> ResolverArenas<'a> {
    fn alloc_module(&'a self, module: ModuleS<'a>) -> Module<'a> {
        self.modules.alloc(module)
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
}

#[derive(PartialEq)]
enum FallbackChecks {
    Everything,
    OnlyTraitAndStatics,
}

impl<'a, 'tcx> Resolver<'a, 'tcx> {
    fn new(session: &'a Session,
           ast_map: &'a hir_map::Map<'tcx>,
           make_glob_map: MakeGlobMap,
           arenas: &'a ResolverArenas<'a>)
           -> Resolver<'a, 'tcx> {
        let root_def_id = ast_map.local_def_id(CRATE_NODE_ID);
        let vis = ty::Visibility::Public;
        let graph_root =
            ModuleS::new(NoParentLink, Some(Def::Mod(root_def_id)), false, vis, arenas);
        let graph_root = arenas.alloc_module(graph_root);

        Resolver {
            session: session,

            ast_map: ast_map,

            // The outermost module has def ID 0; this is not reflected in the
            // AST.
            graph_root: graph_root,

            trait_item_map: FnvHashMap(),
            structs: FnvHashMap(),

            unresolved_imports: 0,

            current_module: graph_root,
            value_ribs: vec![Rib::new(ModuleRibKind(graph_root))],
            type_ribs: vec![Rib::new(ModuleRibKind(graph_root))],
            label_ribs: Vec::new(),

            current_trait_ref: None,
            current_self_type: None,

            primitive_type_table: PrimitiveTypeTable::new(),

            def_map: RefCell::new(NodeMap()),
            freevars: NodeMap(),
            freevars_seen: NodeMap(),
            export_map: NodeMap(),
            trait_map: NodeMap(),
            used_imports: HashSet::new(),
            used_crates: HashSet::new(),

            emit_errors: true,
            make_glob_map: make_glob_map == MakeGlobMap::Yes,
            glob_map: NodeMap(),

            callback: None,
            resolved: false,
            privacy_errors: Vec::new(),

            arenas: arenas,
        }
    }

    fn arenas() -> ResolverArenas<'a> {
        ResolverArenas {
            modules: arena::TypedArena::new(),
            name_bindings: arena::TypedArena::new(),
            import_directives: arena::TypedArena::new(),
            name_resolutions: arena::TypedArena::new(),
        }
    }

    fn new_module(&self,
                  parent_link: ParentLink<'a>,
                  def: Option<Def>,
                  external: bool,
                  vis: ty::Visibility) -> Module<'a> {
        self.arenas.alloc_module(ModuleS::new(parent_link, def, external, vis, self.arenas))
    }

    fn new_extern_crate_module(&self,
                               parent_link: ParentLink<'a>,
                               def: Def,
                               vis: ty::Visibility,
                               local_node_id: NodeId)
                               -> Module<'a> {
        let mut module = ModuleS::new(parent_link, Some(def), false, vis, self.arenas);
        module.extern_crate_id = Some(local_node_id);
        self.arenas.modules.alloc(module)
    }

    fn get_ribs<'b>(&'b mut self, ns: Namespace) -> &'b mut Vec<Rib<'a>> {
        match ns { ValueNS => &mut self.value_ribs, TypeNS => &mut self.type_ribs }
    }

    #[inline]
    fn record_use(&mut self, name: Name, ns: Namespace, binding: &'a NameBinding<'a>) {
        // track extern crates for unused_extern_crate lint
        if let Some(DefId { krate, .. }) = binding.module().and_then(ModuleS::def_id) {
            self.used_crates.insert(krate);
        }

        let (directive, privacy_error) = match binding.kind {
            NameBindingKind::Import { directive, ref privacy_error, .. } =>
                (directive, privacy_error),
            _ => return,
        };

        self.used_imports.insert((directive.id, ns));
        if let Some(error) = privacy_error.as_ref() {
            self.privacy_errors.push((**error).clone());
        }

        if !self.make_glob_map {
            return;
        }
        if self.glob_map.contains_key(&directive.id) {
            self.glob_map.get_mut(&directive.id).unwrap().insert(name);
            return;
        }

        let mut new_set = FnvHashSet();
        new_set.insert(name);
        self.glob_map.insert(directive.id, new_set);
    }

    fn get_trait_name(&self, did: DefId) -> Name {
        if let Some(node_id) = self.ast_map.as_local_node_id(did) {
            self.ast_map.expect_item(node_id).name
        } else {
            self.session.cstore.item_name(did)
        }
    }

    /// Resolves the given module path from the given root `module_`.
    fn resolve_module_path_from_root(&mut self,
                                     module_: Module<'a>,
                                     module_path: &[Name],
                                     index: usize,
                                     span: Span)
                                     -> ResolveResult<Module<'a>> {
        fn search_parent_externals(needle: Name, module: Module) -> Option<Module> {
            match module.resolve_name(needle, TypeNS, false) {
                Success(binding) if binding.is_extern_crate() => Some(module),
                _ => match module.parent_link {
                    ModuleParentLink(ref parent, _) => {
                        search_parent_externals(needle, parent)
                    }
                    _ => None,
                },
            }
        }

        let mut search_module = module_;
        let mut index = index;
        let module_path_len = module_path.len();

        // Resolve the module part of the path. This does not involve looking
        // upward though scope chains; we simply resolve names directly in
        // modules as we go.
        while index < module_path_len {
            let name = module_path[index];
            match self.resolve_name_in_module(search_module, name, TypeNS, false, true) {
                Failed(None) => {
                    let segment_name = name.as_str();
                    let module_name = module_to_string(search_module);
                    let mut span = span;
                    let msg = if "???" == &module_name {
                        span.hi = span.lo + Pos::from_usize(segment_name.len());

                        match search_parent_externals(name, &self.current_module) {
                            Some(module) => {
                                let path_str = names_to_string(module_path);
                                let target_mod_str = module_to_string(&module);
                                let current_mod_str = module_to_string(&self.current_module);

                                let prefix = if target_mod_str == current_mod_str {
                                    "self::".to_string()
                                } else {
                                    format!("{}::", target_mod_str)
                                };

                                format!("Did you mean `{}{}`?", prefix, path_str)
                            }
                            None => format!("Maybe a missing `extern crate {}`?", segment_name),
                        }
                    } else {
                        format!("Could not find `{}` in `{}`", segment_name, module_name)
                    };

                    return Failed(Some((span, msg)));
                }
                Failed(err) => return Failed(err),
                Indeterminate => {
                    debug!("(resolving module path for import) module resolution is \
                            indeterminate: {}",
                           name);
                    return Indeterminate;
                }
                Success(binding) => {
                    // Check to see whether there are type bindings, and, if
                    // so, whether there is a module within.
                    if let Some(module_def) = binding.module() {
                        self.check_privacy(name, binding, span);
                        search_module = module_def;
                    } else {
                        let msg = format!("Not a module `{}`", name);
                        return Failed(Some((span, msg)));
                    }
                }
            }

            index += 1;
        }

        return Success(search_module);
    }

    /// Attempts to resolve the module part of an import directive or path
    /// rooted at the given module.
    fn resolve_module_path(&mut self,
                           module_path: &[Name],
                           use_lexical_scope: UseLexicalScopeFlag,
                           span: Span)
                           -> ResolveResult<Module<'a>> {
        if module_path.len() == 0 {
            return Success(self.graph_root) // Use the crate root
        }

        debug!("(resolving module path for import) processing `{}` rooted at `{}`",
               names_to_string(module_path),
               module_to_string(self.current_module));

        // Resolve the module prefix, if any.
        let module_prefix_result = self.resolve_module_prefix(module_path, span);

        let search_module;
        let start_index;
        match module_prefix_result {
            Failed(err) => return Failed(err),
            Indeterminate => {
                debug!("(resolving module path for import) indeterminate; bailing");
                return Indeterminate;
            }
            Success(NoPrefixFound) => {
                // There was no prefix, so we're considering the first element
                // of the path. How we handle this depends on whether we were
                // instructed to use lexical scope or not.
                match use_lexical_scope {
                    DontUseLexicalScope => {
                        // This is a crate-relative path. We will start the
                        // resolution process at index zero.
                        search_module = self.graph_root;
                        start_index = 0;
                    }
                    UseLexicalScope => {
                        // This is not a crate-relative path. We resolve the
                        // first component of the path in the current lexical
                        // scope and then proceed to resolve below that.
                        let ident = hir::Ident::from_name(module_path[0]);
                        match self.resolve_ident_in_lexical_scope(ident, TypeNS, true)
                                  .and_then(LexicalScopeBinding::module) {
                            None => return Failed(None),
                            Some(containing_module) => {
                                search_module = containing_module;
                                start_index = 1;
                            }
                        }
                    }
                }
            }
            Success(PrefixFound(ref containing_module, index)) => {
                search_module = containing_module;
                start_index = index;
            }
        }

        self.resolve_module_path_from_root(search_module,
                                           module_path,
                                           start_index,
                                           span)
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
                                      ident: hir::Ident,
                                      ns: Namespace,
                                      record_used: bool)
                                      -> Option<LexicalScopeBinding<'a>> {
        let name = match ns { ValueNS => ident.name, TypeNS => ident.unhygienic_name };

        // Walk backwards up the ribs in scope.
        for i in (0 .. self.get_ribs(ns).len()).rev() {
            if let Some(def) = self.get_ribs(ns)[i].bindings.get(&name).cloned() {
                // The ident resolves to a type parameter or local variable.
                return Some(LexicalScopeBinding::LocalDef(LocalDef {
                    ribs: Some((ns, i)),
                    def: def,
                }));
            }

            if let ModuleRibKind(module) = self.get_ribs(ns)[i].kind {
                let name = ident.unhygienic_name;
                let item = self.resolve_name_in_module(module, name, ns, true, record_used);
                if let Success(binding) = item {
                    // The ident resolves to an item.
                    return Some(LexicalScopeBinding::Item(binding));
                }

                // We can only see through anonymous modules
                if module.def.is_some() { return None; }
            }
        }

        None
    }

    /// Returns the nearest normal module parent of the given module.
    fn get_nearest_normal_module_parent(&self, module_: Module<'a>) -> Option<Module<'a>> {
        let mut module_ = module_;
        loop {
            match module_.parent_link {
                NoParentLink => return None,
                ModuleParentLink(new_module, _) |
                BlockParentLink(new_module, _) => {
                    let new_module = new_module;
                    if new_module.is_normal() {
                        return Some(new_module);
                    }
                    module_ = new_module;
                }
            }
        }
    }

    /// Returns the nearest normal module parent of the given module, or the
    /// module itself if it is a normal module.
    fn get_nearest_normal_module_parent_or_self(&self, module_: Module<'a>) -> Module<'a> {
        if module_.is_normal() {
            return module_;
        }
        match self.get_nearest_normal_module_parent(module_) {
            None => module_,
            Some(new_module) => new_module,
        }
    }

    /// Resolves a "module prefix". A module prefix is one or both of (a) `self::`;
    /// (b) some chain of `super::`.
    /// grammar: (SELF MOD_SEP ) ? (SUPER MOD_SEP) *
    fn resolve_module_prefix(&mut self, module_path: &[Name], span: Span)
                             -> ResolveResult<ModulePrefixResult<'a>> {
        // Start at the current module if we see `self` or `super`, or at the
        // top of the crate otherwise.
        let mut i = match &*module_path[0].as_str() {
            "self" => 1,
            "super" => 0,
            _ => return Success(NoPrefixFound),
        };
        let module_ = self.current_module;
        let mut containing_module = self.get_nearest_normal_module_parent_or_self(module_);

        // Now loop through all the `super`s we find.
        while i < module_path.len() && "super" == module_path[i].as_str() {
            debug!("(resolving module prefix) resolving `super` at {}",
                   module_to_string(&containing_module));
            match self.get_nearest_normal_module_parent(containing_module) {
                None => {
                    let msg = "There are too many initial `super`s.".into();
                    return Failed(Some((span, msg)));
                }
                Some(new_module) => {
                    containing_module = new_module;
                    i += 1;
                }
            }
        }

        debug!("(resolving module prefix) finished resolving prefix at {}",
               module_to_string(&containing_module));

        return Success(PrefixFound(containing_module, i));
    }

    /// Attempts to resolve the supplied name in the given module for the
    /// given namespace. If successful, returns the binding corresponding to
    /// the name.
    fn resolve_name_in_module(&mut self,
                              module: Module<'a>,
                              name: Name,
                              namespace: Namespace,
                              use_lexical_scope: bool,
                              record_used: bool)
                              -> ResolveResult<&'a NameBinding<'a>> {
        debug!("(resolving name in module) resolving `{}` in `{}`", name, module_to_string(module));

        self.populate_module_if_necessary(module);
        match use_lexical_scope {
            true => module.resolve_name_in_lexical_scope(name, namespace)
                          .map(Success).unwrap_or(Failed(None)),
            false => module.resolve_name(name, namespace, false),
        }.and_then(|binding| {
            if record_used {
                self.record_use(name, namespace, binding);
            }
            Success(binding)
        })
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
        if let Some(module) = self.current_module.module_children.borrow().get(&id) {
            // Move down in the graph.
            let orig_module = ::std::mem::replace(&mut self.current_module, module);
            self.value_ribs.push(Rib::new(ModuleRibKind(module)));
            self.type_ribs.push(Rib::new(ModuleRibKind(module)));

            f(self);

            self.current_module = orig_module;
            self.value_ribs.pop();
            self.type_ribs.pop();
        } else {
            f(self);
        }
    }

    /// Searches the current set of local scopes for labels.
    /// Stops after meeting a closure.
    fn search_label(&self, name: Name) -> Option<Def> {
        for rib in self.label_ribs.iter().rev() {
            match rib.kind {
                NormalRibKind => {
                    // Continue
                }
                _ => {
                    // Do not resolve labels across function boundary
                    return None;
                }
            }
            let result = rib.bindings.get(&name).cloned();
            if result.is_some() {
                return result;
            }
        }
        None
    }

    fn resolve_crate(&mut self, krate: &hir::Crate) {
        debug!("(resolving crate) starting");
        self.current_module = self.graph_root;
        intravisit::walk_crate(self, krate);
    }

    fn resolve_item(&mut self, item: &Item) {
        let name = item.name;

        debug!("(resolving item) resolving {}", name);

        match item.node {
            ItemEnum(_, ref generics) |
            ItemTy(_, ref generics) |
            ItemStruct(_, ref generics) => {
                self.with_type_parameter_rib(HasTypeParameters(generics, TypeSpace, ItemRibKind),
                                             |this| intravisit::walk_item(this, item));
            }
            ItemFn(_, _, _, _, ref generics, _) => {
                self.with_type_parameter_rib(HasTypeParameters(generics, FnSpace, ItemRibKind),
                                             |this| intravisit::walk_item(this, item));
            }

            ItemDefaultImpl(_, ref trait_ref) => {
                self.with_optional_trait_ref(Some(trait_ref), |_, _| {});
            }
            ItemImpl(_, _, ref generics, ref opt_trait_ref, ref self_type, ref impl_items) => {
                self.resolve_implementation(generics,
                                            opt_trait_ref,
                                            &self_type,
                                            item.id,
                                            impl_items);
            }

            ItemTrait(_, ref generics, ref bounds, ref trait_items) => {
                // Create a new rib for the trait-wide type parameters.
                self.with_type_parameter_rib(HasTypeParameters(generics,
                                                               TypeSpace,
                                                               ItemRibKind),
                                             |this| {
                    let local_def_id = this.ast_map.local_def_id(item.id);
                    this.with_self_rib(Def::SelfTy(Some(local_def_id), None), |this| {
                        this.visit_generics(generics);
                        walk_list!(this, visit_ty_param_bound, bounds);

                        for trait_item in trait_items {
                            match trait_item.node {
                                hir::ConstTraitItem(_, ref default) => {
                                    // Only impose the restrictions of
                                    // ConstRibKind if there's an actual constant
                                    // expression in a provided default.
                                    if default.is_some() {
                                        this.with_constant_rib(|this| {
                                            intravisit::walk_trait_item(this, trait_item)
                                        });
                                    } else {
                                        intravisit::walk_trait_item(this, trait_item)
                                    }
                                }
                                hir::MethodTraitItem(ref sig, _) => {
                                    let type_parameters =
                                        HasTypeParameters(&sig.generics,
                                                          FnSpace,
                                                          MethodRibKind);
                                    this.with_type_parameter_rib(type_parameters, |this| {
                                        intravisit::walk_trait_item(this, trait_item)
                                    });
                                }
                                hir::TypeTraitItem(..) => {
                                    this.with_type_parameter_rib(NoTypeParameters, |this| {
                                        intravisit::walk_trait_item(this, trait_item)
                                    });
                                }
                            };
                        }
                    });
                });
            }

            ItemMod(_) | ItemForeignMod(_) => {
                self.with_scope(item.id, |this| {
                    intravisit::walk_item(this, item);
                });
            }

            ItemConst(..) | ItemStatic(..) => {
                self.with_constant_rib(|this| {
                    intravisit::walk_item(this, item);
                });
            }

            ItemUse(ref view_path) => {
                match view_path.node {
                    hir::ViewPathList(ref prefix, ref items) => {
                        // Resolve prefix of an import with empty braces (issue #28388)
                        if items.is_empty() && !prefix.segments.is_empty() {
                            match self.resolve_crate_relative_path(prefix.span,
                                                                   &prefix.segments,
                                                                   TypeNS) {
                                Ok(def) =>
                                    self.record_def(item.id, PathResolution::new(def, 0)),
                                Err(true) => self.record_def(item.id, err_path_resolution()),
                                Err(false) => {
                                    resolve_error(self,
                                                  prefix.span,
                                                  ResolutionError::FailedToResolve(
                                                      &path_names_to_string(prefix, 0)));
                                    self.record_def(item.id, err_path_resolution());
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            ItemExternCrate(_) => {
                // do nothing, these are just around to be encoded
            }
        }
    }

    fn with_type_parameter_rib<'b, F>(&'b mut self, type_parameters: TypeParameters<'a, 'b>, f: F)
        where F: FnOnce(&mut Resolver)
    {
        match type_parameters {
            HasTypeParameters(generics, space, rib_kind) => {
                let mut function_type_rib = Rib::new(rib_kind);
                let mut seen_bindings = HashSet::new();
                for (index, type_parameter) in generics.ty_params.iter().enumerate() {
                    let name = type_parameter.name;
                    debug!("with_type_parameter_rib: {}", type_parameter.id);

                    if seen_bindings.contains(&name) {
                        resolve_error(self,
                                      type_parameter.span,
                                      ResolutionError::NameAlreadyUsedInTypeParameterList(name));
                    }
                    seen_bindings.insert(name);

                    // plain insert (no renaming)
                    let def_id = self.ast_map.local_def_id(type_parameter.id);
                    let def = Def::TyParam(space, index as u32, def_id, name);
                    function_type_rib.bindings.insert(name, def);
                }
                self.type_ribs.push(function_type_rib);
            }

            NoTypeParameters => {
                // Nothing to do.
            }
        }

        f(self);

        match type_parameters {
            HasTypeParameters(..) => {
                if !self.resolved {
                    self.type_ribs.pop();
                }
            }
            NoTypeParameters => {}
        }
    }

    fn with_label_rib<F>(&mut self, f: F)
        where F: FnOnce(&mut Resolver)
    {
        self.label_ribs.push(Rib::new(NormalRibKind));
        f(self);
        if !self.resolved {
            self.label_ribs.pop();
        }
    }

    fn with_constant_rib<F>(&mut self, f: F)
        where F: FnOnce(&mut Resolver)
    {
        self.value_ribs.push(Rib::new(ConstantItemRibKind));
        self.type_ribs.push(Rib::new(ConstantItemRibKind));
        f(self);
        if !self.resolved {
            self.type_ribs.pop();
            self.value_ribs.pop();
        }
    }

    fn resolve_function(&mut self, rib_kind: RibKind<'a>, declaration: &FnDecl, block: &Block) {
        // Create a value rib for the function.
        self.value_ribs.push(Rib::new(rib_kind));

        // Create a label rib for the function.
        self.label_ribs.push(Rib::new(rib_kind));

        // Add each argument to the rib.
        let mut bindings_list = HashMap::new();
        for argument in &declaration.inputs {
            self.resolve_pattern(&argument.pat, ArgumentIrrefutableMode, &mut bindings_list);

            self.visit_ty(&argument.ty);

            debug!("(resolving function) recorded argument");
        }
        intravisit::walk_fn_ret_ty(self, &declaration.output);

        // Resolve the function body.
        self.visit_block(block);

        debug!("(resolving function) leaving function");

        if !self.resolved {
            self.label_ribs.pop();
            self.value_ribs.pop();
        }
    }

    fn resolve_trait_reference(&mut self,
                               id: NodeId,
                               trait_path: &Path,
                               path_depth: usize)
                               -> Result<PathResolution, ()> {
        self.resolve_path(id, trait_path, path_depth, TypeNS).and_then(|path_res| {
            if let Def::Trait(_) = path_res.base_def {
                debug!("(resolving trait) found trait def: {:?}", path_res);
                Ok(path_res)
            } else {
                let mut err =
                    resolve_struct_error(self,
                                  trait_path.span,
                                  ResolutionError::IsNotATrait(&path_names_to_string(trait_path,
                                                                                      path_depth)));

                // If it's a typedef, give a note
                if let Def::TyAlias(did) = path_res.base_def {
                    err.fileline_note(trait_path.span,
                                  "`type` aliases cannot be used for traits");
                    if let Some(sp) = self.ast_map.span_if_local(did) {
                        err.span_note(sp, "type defined here");
                    }
                }
                err.emit();
                Err(true)
            }
        }).map_err(|error_reported| {
            if error_reported { return }

            // find possible candidates
            let trait_name = trait_path.segments.last().unwrap().identifier.name;
            let candidates =
                self.lookup_candidates(
                    trait_name,
                    TypeNS,
                    |def| match def {
                        Def::Trait(_) => true,
                        _             => false,
                    },
                );

            // create error object
            let name = &path_names_to_string(trait_path, path_depth);
            let error =
                ResolutionError::UndeclaredTraitName(
                    name,
                    candidates,
                );

            resolve_error(self, trait_path.span, error);
        })
    }

    fn resolve_generics(&mut self, generics: &Generics) {
        for predicate in &generics.where_clause.predicates {
            match predicate {
                &hir::WherePredicate::BoundPredicate(_) |
                &hir::WherePredicate::RegionPredicate(_) => {}
                &hir::WherePredicate::EqPredicate(ref eq_pred) => {
                    self.resolve_path(eq_pred.id, &eq_pred.path, 0, TypeNS).and_then(|path_res| {
                        if let PathResolution { base_def: Def::TyParam(..), .. } = path_res {
                            Ok(self.record_def(eq_pred.id, path_res))
                        } else {
                            Err(false)
                        }
                    }).map_err(|error_reported| {
                        self.record_def(eq_pred.id, err_path_resolution());
                        if error_reported { return }
                        let error_variant = ResolutionError::UndeclaredAssociatedType;
                        resolve_error(self, eq_pred.span, error_variant);
                    }).unwrap_or(());
                }
            }
        }
        intravisit::walk_generics(self, generics);
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
            if let Ok(path_res) = self.resolve_trait_reference(trait_ref.ref_id,
                                                               &trait_ref.path,
                                                               0) {
                assert!(path_res.depth == 0);
                self.record_def(trait_ref.ref_id, path_res);
                new_val = Some((path_res.base_def.def_id(), trait_ref.clone()));
                new_id = Some(path_res.base_def.def_id());
            } else {
                self.record_def(trait_ref.ref_id, err_path_resolution());
            }
            intravisit::walk_trait_ref(self, trait_ref);
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
        let name = special_names::type_self;
        self_type_rib.bindings.insert(name, self_def);
        self.type_ribs.push(self_type_rib);
        f(self);
        if !self.resolved {
            self.type_ribs.pop();
        }
    }

    fn resolve_implementation(&mut self,
                              generics: &Generics,
                              opt_trait_reference: &Option<TraitRef>,
                              self_type: &Ty,
                              item_id: NodeId,
                              impl_items: &[ImplItem]) {
        // If applicable, create a rib for the type parameters.
        self.with_type_parameter_rib(HasTypeParameters(generics,
                                                       TypeSpace,
                                                       ItemRibKind),
                                     |this| {
            // Resolve the type parameters.
            this.visit_generics(generics);

            // Resolve the trait reference, if necessary.
            this.with_optional_trait_ref(opt_trait_reference.as_ref(), |this, trait_id| {
                // Resolve the self type.
                this.visit_ty(self_type);

                this.with_self_rib(Def::SelfTy(trait_id, Some((item_id, self_type.id))), |this| {
                    this.with_current_self_type(self_type, |this| {
                        for impl_item in impl_items {
                            this.resolve_visibility(&impl_item.vis);
                            match impl_item.node {
                                hir::ImplItemKind::Const(..) => {
                                    // If this is a trait impl, ensure the const
                                    // exists in trait
                                    this.check_trait_item(impl_item.name,
                                                          impl_item.span,
                                        |n, s| ResolutionError::ConstNotMemberOfTrait(n, s));
                                    this.with_constant_rib(|this| {
                                        intravisit::walk_impl_item(this, impl_item);
                                    });
                                }
                                hir::ImplItemKind::Method(ref sig, _) => {
                                    // If this is a trait impl, ensure the method
                                    // exists in trait
                                    this.check_trait_item(impl_item.name,
                                                          impl_item.span,
                                        |n, s| ResolutionError::MethodNotMemberOfTrait(n, s));

                                    // We also need a new scope for the method-
                                    // specific type parameters.
                                    let type_parameters =
                                        HasTypeParameters(&sig.generics,
                                                          FnSpace,
                                                          MethodRibKind);
                                    this.with_type_parameter_rib(type_parameters, |this| {
                                        intravisit::walk_impl_item(this, impl_item);
                                    });
                                }
                                hir::ImplItemKind::Type(ref ty) => {
                                    // If this is a trait impl, ensure the type
                                    // exists in trait
                                    this.check_trait_item(impl_item.name,
                                                          impl_item.span,
                                        |n, s| ResolutionError::TypeNotMemberOfTrait(n, s));

                                    this.visit_ty(ty);
                                }
                            }
                        }
                    });
                });
            });
        });
    }

    fn check_trait_item<F>(&self, name: Name, span: Span, err: F)
        where F: FnOnce(Name, &str) -> ResolutionError
    {
        // If there is a TraitRef in scope for an impl, then the method must be in the
        // trait.
        if let Some((did, ref trait_ref)) = self.current_trait_ref {
            if !self.trait_item_map.contains_key(&(name, did)) {
                let path_str = path_names_to_string(&trait_ref.path, 0);
                resolve_error(self, span, err(name, &path_str));
            }
        }
    }

    fn resolve_local(&mut self, local: &Local) {
        // Resolve the type.
        walk_list!(self, visit_ty, &local.ty);

        // Resolve the initializer.
        walk_list!(self, visit_expr, &local.init);

        // Resolve the pattern.
        self.resolve_pattern(&local.pat, LocalIrrefutableMode, &mut HashMap::new());
    }

    // build a map from pattern identifiers to binding-info's.
    // this is done hygienically. This could arise for a macro
    // that expands into an or-pattern where one 'x' was from the
    // user and one 'x' came from the macro.
    fn binding_mode_map(&mut self, pat: &Pat) -> BindingMap {
        let mut result = HashMap::new();
        pat_bindings(&self.def_map, pat, |binding_mode, _id, sp, path1| {
            let name = path1.node;
            result.insert(name,
                          BindingInfo {
                              span: sp,
                              binding_mode: binding_mode,
                          });
        });
        return result;
    }

    // check that all of the arms in an or-pattern have exactly the
    // same set of bindings, with the same binding modes for each.
    fn check_consistent_bindings(&mut self, arm: &Arm) {
        if arm.pats.is_empty() {
            return;
        }
        let map_0 = self.binding_mode_map(&arm.pats[0]);
        for (i, p) in arm.pats.iter().enumerate() {
            let map_i = self.binding_mode_map(&p);

            for (&key, &binding_0) in &map_0 {
                match map_i.get(&key) {
                    None => {
                        resolve_error(self,
                                      p.span,
                                      ResolutionError::VariableNotBoundInPattern(key, i + 1));
                    }
                    Some(binding_i) => {
                        if binding_0.binding_mode != binding_i.binding_mode {
                            resolve_error(self,
                                          binding_i.span,
                                          ResolutionError::VariableBoundWithDifferentMode(key,
                                                                                          i + 1));
                        }
                    }
                }
            }

            for (&key, &binding) in &map_i {
                if !map_0.contains_key(&key) {
                    resolve_error(self,
                                  binding.span,
                                  ResolutionError::VariableNotBoundInParentPattern(key, i + 1));
                }
            }
        }
    }

    fn resolve_arm(&mut self, arm: &Arm) {
        self.value_ribs.push(Rib::new(NormalRibKind));

        let mut bindings_list = HashMap::new();
        for pattern in &arm.pats {
            self.resolve_pattern(&pattern, RefutableMode, &mut bindings_list);
        }

        // This has to happen *after* we determine which
        // pat_idents are variants
        self.check_consistent_bindings(arm);

        walk_list!(self, visit_expr, &arm.guard);
        self.visit_expr(&arm.body);

        if !self.resolved {
            self.value_ribs.pop();
        }
    }

    fn resolve_block(&mut self, block: &Block) {
        debug!("(resolving block) entering block");
        // Move down in the graph, if there's an anonymous module rooted here.
        let orig_module = self.current_module;
        let anonymous_module =
            orig_module.module_children.borrow().get(&block.id).map(|module| *module);

        if let Some(anonymous_module) = anonymous_module {
            debug!("(resolving block) found anonymous module, moving down");
            self.value_ribs.push(Rib::new(ModuleRibKind(anonymous_module)));
            self.type_ribs.push(Rib::new(ModuleRibKind(anonymous_module)));
            self.current_module = anonymous_module;
        } else {
            self.value_ribs.push(Rib::new(NormalRibKind));
        }

        // Descend into the block.
        intravisit::walk_block(self, block);

        // Move back up.
        if !self.resolved {
            self.current_module = orig_module;
            self.value_ribs.pop();
            if let Some(_) = anonymous_module {
                self.type_ribs.pop();
            }
        }
        debug!("(resolving block) leaving block");
    }

    fn resolve_type(&mut self, ty: &Ty) {
        match ty.node {
            TyPath(ref maybe_qself, ref path) => {
                let resolution = match self.resolve_possibly_assoc_item(ty.id,
                                                                        maybe_qself.as_ref(),
                                                                        path,
                                                                        TypeNS) {
                    // `<T>::a::b::c` is resolved by typeck alone.
                    TypecheckRequired => {
                        // Resolve embedded types.
                        intravisit::walk_ty(self, ty);
                        return;
                    }
                    ResolveAttempt(resolution) => resolution,
                };

                // This is a path in the type namespace. Walk through scopes
                // looking for it.
                if let Some(def) = resolution {
                    // Write the result into the def map.
                    debug!("(resolving type) writing resolution for `{}` (id {}) = {:?}",
                           path_names_to_string(path, 0), ty.id, def);
                    self.record_def(ty.id, def);
                } else {
                    self.record_def(ty.id, err_path_resolution());

                    // Keep reporting some errors even if they're ignored above.
                    if let Err(true) = self.resolve_path(ty.id, path, 0, TypeNS) {
                        // `resolve_path` already reported the error
                    } else {
                        let kind = if maybe_qself.is_some() {
                            "associated type"
                        } else {
                            "type name"
                        };

                        let self_type_name = special_idents::type_self.name;
                        let is_invalid_self_type_name = path.segments.len() > 0 &&
                                                        maybe_qself.is_none() &&
                                                        path.segments[0].identifier.name ==
                                                        self_type_name;
                        if is_invalid_self_type_name {
                            resolve_error(self,
                                          ty.span,
                                          ResolutionError::SelfUsedOutsideImplOrTrait);
                        } else {
                            let segment = path.segments.last();
                            let segment = segment.expect("missing name in path");
                            let type_name = segment.identifier.name;

                            let candidates =
                                self.lookup_candidates(
                                    type_name,
                                    TypeNS,
                                    |def| match def {
                                        Def::Trait(_) |
                                        Def::Enum(_) |
                                        Def::Struct(_) |
                                        Def::TyAlias(_) => true,
                                        _               => false,
                                    },
                                );

                            // create error object
                            let name = &path_names_to_string(path, 0);
                            let error =
                                ResolutionError::UseOfUndeclared(
                                    kind,
                                    name,
                                    candidates,
                                );

                            resolve_error(self, ty.span, error);
                        }
                    }
                }
            }
            _ => {}
        }
        // Resolve embedded types.
        intravisit::walk_ty(self, ty);
    }

    fn resolve_pattern(&mut self,
                       pattern: &Pat,
                       mode: PatternBindingMode,
                       // Maps idents to the node ID for the (outermost)
                       // pattern that binds them
                       bindings_list: &mut HashMap<Name, NodeId>) {
        let pat_id = pattern.id;
        pattern.walk(|pattern| {
            match pattern.node {
                PatKind::Ident(binding_mode, ref path1, ref at_rhs) => {
                    // The meaning of PatKind::Ident with no type parameters
                    // depends on whether an enum variant or unit-like struct
                    // with that name is in scope. The probing lookup has to
                    // be careful not to emit spurious errors. Only matching
                    // patterns (match) can match nullary variants or
                    // unit-like structs. For binding patterns (let
                    // and the LHS of @-patterns), matching such a value is
                    // simply disallowed (since it's rarely what you want).
                    let const_ok = mode == RefutableMode && at_rhs.is_none();

                    let ident = path1.node;
                    let renamed = ident.name;

                    match self.resolve_bare_identifier_pattern(ident, pattern.span) {
                        FoundStructOrEnumVariant(def) if const_ok => {
                            debug!("(resolving pattern) resolving `{}` to struct or enum variant",
                                   renamed);

                            self.enforce_default_binding_mode(pattern,
                                                              binding_mode,
                                                              "an enum variant");
                            self.record_def(pattern.id,
                                            PathResolution {
                                                base_def: def,
                                                depth: 0,
                                            });
                        }
                        FoundStructOrEnumVariant(..) => {
                            resolve_error(
                                self,
                                pattern.span,
                                ResolutionError::DeclarationShadowsEnumVariantOrUnitLikeStruct(
                                    renamed)
                            );
                            self.record_def(pattern.id, err_path_resolution());
                        }
                        FoundConst(def, _) if const_ok => {
                            debug!("(resolving pattern) resolving `{}` to constant", renamed);

                            self.enforce_default_binding_mode(pattern, binding_mode, "a constant");
                            self.record_def(pattern.id,
                                            PathResolution {
                                                base_def: def,
                                                depth: 0,
                                            });
                        }
                        FoundConst(def, name) => {
                            resolve_error(
                                self,
                                pattern.span,
                                ResolutionError::OnlyIrrefutablePatternsAllowedHere(def.def_id(),
                                                                                    name)
                            );
                            self.record_def(pattern.id, err_path_resolution());
                        }
                        BareIdentifierPatternUnresolved => {
                            debug!("(resolving pattern) binding `{}`", renamed);

                            let def_id = self.ast_map.local_def_id(pattern.id);
                            let def = Def::Local(def_id, pattern.id);

                            // Record the definition so that later passes
                            // will be able to distinguish variants from
                            // locals in patterns.

                            self.record_def(pattern.id,
                                            PathResolution {
                                                base_def: def,
                                                depth: 0,
                                            });

                            // Add the binding to the local ribs, if it
                            // doesn't already exist in the bindings list. (We
                            // must not add it if it's in the bindings list
                            // because that breaks the assumptions later
                            // passes make about or-patterns.)
                            if !bindings_list.contains_key(&renamed) {
                                let this = &mut *self;
                                let last_rib = this.value_ribs.last_mut().unwrap();
                                last_rib.bindings.insert(renamed, def);
                                bindings_list.insert(renamed, pat_id);
                            } else if mode == ArgumentIrrefutableMode &&
                               bindings_list.contains_key(&renamed) {
                                // Forbid duplicate bindings in the same
                                // parameter list.
                                resolve_error(
                                    self,
                                    pattern.span,
                                    ResolutionError::IdentifierBoundMoreThanOnceInParameterList(
                                        &ident.name.as_str())
                                );
                            } else if bindings_list.get(&renamed) == Some(&pat_id) {
                                // Then this is a duplicate variable in the
                                // same disjunction, which is an error.
                                resolve_error(
                                    self,
                                    pattern.span,
                                    ResolutionError::IdentifierBoundMoreThanOnceInSamePattern(
                                        &ident.name.as_str())
                                );
                            }
                            // Else, not bound in the same pattern: do
                            // nothing.
                        }
                    }
                }

                PatKind::TupleStruct(ref path, _) | PatKind::Path(ref path) => {
                    // This must be an enum variant, struct or const.
                    let resolution = match self.resolve_possibly_assoc_item(pat_id,
                                                                            None,
                                                                            path,
                                                                            ValueNS) {
                        // The below shouldn't happen because all
                        // qualified paths should be in PatKind::QPath.
                        TypecheckRequired =>
                            span_bug!(path.span,
                                      "resolve_possibly_assoc_item claimed that a path \
                                       in PatKind::Path or PatKind::TupleStruct \
                                       requires typecheck to resolve, but qualified \
                                       paths should be PatKind::QPath"),
                        ResolveAttempt(resolution) => resolution,
                    };
                    if let Some(path_res) = resolution {
                        match path_res.base_def {
                            Def::Struct(..) if path_res.depth == 0 => {
                                self.record_def(pattern.id, path_res);
                            }
                            Def::Variant(..) | Def::Const(..) => {
                                self.record_def(pattern.id, path_res);
                            }
                            Def::Static(..) => {
                                resolve_error(&self,
                                              path.span,
                                              ResolutionError::StaticVariableReference);
                                self.record_def(pattern.id, err_path_resolution());
                            }
                            _ => {
                                // If anything ends up here entirely resolved,
                                // it's an error. If anything ends up here
                                // partially resolved, that's OK, because it may
                                // be a `T::CONST` that typeck will resolve.
                                if path_res.depth == 0 {
                                    resolve_error(
                                        self,
                                        path.span,
                                        ResolutionError::NotAnEnumVariantStructOrConst(
                                            &path.segments
                                                 .last()
                                                 .unwrap()
                                                 .identifier
                                                 .name
                                                 .as_str())
                                    );
                                    self.record_def(pattern.id, err_path_resolution());
                                } else {
                                    let const_name = path.segments
                                                         .last()
                                                         .unwrap()
                                                         .identifier
                                                         .name;
                                    let traits = self.get_traits_containing_item(const_name);
                                    self.trait_map.insert(pattern.id, traits);
                                    self.record_def(pattern.id, path_res);
                                }
                            }
                        }
                    } else if let Err(false) = self.resolve_path(pat_id, &path, 0, ValueNS) {
                        resolve_error(
                            self,
                            path.span,
                            ResolutionError::UnresolvedEnumVariantStructOrConst(
                                &path.segments.last().unwrap().identifier.name.as_str())
                        );
                        self.record_def(pattern.id, err_path_resolution());
                    }
                    intravisit::walk_path(self, path);
                }

                PatKind::QPath(ref qself, ref path) => {
                    // Associated constants only.
                    let resolution = match self.resolve_possibly_assoc_item(pat_id,
                                                                            Some(qself),
                                                                            path,
                                                                            ValueNS) {
                        TypecheckRequired => {
                            // All `<T>::CONST` should end up here, and will
                            // require use of the trait map to resolve
                            // during typechecking.
                            let const_name = path.segments
                                                 .last()
                                                 .unwrap()
                                                 .identifier
                                                 .name;
                            let traits = self.get_traits_containing_item(const_name);
                            self.trait_map.insert(pattern.id, traits);
                            intravisit::walk_pat(self, pattern);
                            return true;
                        }
                        ResolveAttempt(resolution) => resolution,
                    };
                    if let Some(path_res) = resolution {
                        match path_res.base_def {
                            // All `<T as Trait>::CONST` should end up here, and
                            // have the trait already selected.
                            Def::AssociatedConst(..) => {
                                self.record_def(pattern.id, path_res);
                            }
                            _ => {
                                resolve_error(
                                    self,
                                    path.span,
                                    ResolutionError::NotAnAssociatedConst(
                                        &path.segments.last().unwrap().identifier.name.as_str()
                                    )
                                );
                                self.record_def(pattern.id, err_path_resolution());
                            }
                        }
                    } else {
                        resolve_error(self,
                                      path.span,
                                      ResolutionError::UnresolvedAssociatedConst(&path.segments
                                                                                      .last()
                                                                                      .unwrap()
                                                                                      .identifier
                                                                                      .name
                                                                                      .as_str()));
                        self.record_def(pattern.id, err_path_resolution());
                    }
                    intravisit::walk_pat(self, pattern);
                }

                PatKind::Struct(ref path, _, _) => {
                    match self.resolve_path(pat_id, path, 0, TypeNS) {
                        Ok(definition) => {
                            self.record_def(pattern.id, definition);
                        }
                        Err(true) => self.record_def(pattern.id, err_path_resolution()),
                        Err(false) => {
                            resolve_error(
                                self,
                                path.span,
                                ResolutionError::DoesNotNameAStruct(
                                    &path_names_to_string(path, 0))
                            );
                            self.record_def(pattern.id, err_path_resolution());
                        }
                    }
                    intravisit::walk_path(self, path);
                }

                PatKind::Lit(_) | PatKind::Range(..) => {
                    intravisit::walk_pat(self, pattern);
                }

                _ => {
                    // Nothing to do.
                }
            }
            true
        });
    }

    fn resolve_bare_identifier_pattern(&mut self, ident: hir::Ident, span: Span)
                                       -> BareIdentifierPatternResolution {
        match self.resolve_ident_in_lexical_scope(ident, ValueNS, true)
                  .map(LexicalScopeBinding::def) {
            Some(def @ Def::Variant(..)) | Some(def @ Def::Struct(..)) => {
                FoundStructOrEnumVariant(def)
            }
            Some(def @ Def::Const(..)) | Some(def @ Def::AssociatedConst(..)) => {
                FoundConst(def, ident.unhygienic_name)
            }
            Some(Def::Static(..)) => {
                resolve_error(self, span, ResolutionError::StaticVariableReference);
                BareIdentifierPatternUnresolved
            }
            _ => BareIdentifierPatternUnresolved,
        }
    }

    /// Handles paths that may refer to associated items
    fn resolve_possibly_assoc_item(&mut self,
                                   id: NodeId,
                                   maybe_qself: Option<&hir::QSelf>,
                                   path: &Path,
                                   namespace: Namespace)
                                   -> AssocItemResolveResult {
        let max_assoc_types;

        match maybe_qself {
            Some(qself) => {
                if qself.position == 0 {
                    return TypecheckRequired;
                }
                max_assoc_types = path.segments.len() - qself.position;
                // Make sure the trait is valid.
                let _ = self.resolve_trait_reference(id, path, max_assoc_types);
            }
            None => {
                max_assoc_types = path.segments.len();
            }
        }

        let mut resolution = self.with_no_errors(|this| {
            this.resolve_path(id, path, 0, namespace).ok()
        });
        for depth in 1..max_assoc_types {
            if resolution.is_some() {
                break;
            }
            self.with_no_errors(|this| {
                resolution = this.resolve_path(id, path, depth, TypeNS).ok();
            });
        }
        if let Some(Def::Mod(_)) = resolution.map(|r| r.base_def) {
            // A module is not a valid type or value.
            resolution = None;
        }
        ResolveAttempt(resolution)
    }

    /// Skips `path_depth` trailing segments, which is also reflected in the
    /// returned value. See `hir::def::PathResolution` for more info.
    fn resolve_path(&mut self, id: NodeId, path: &Path, path_depth: usize, namespace: Namespace)
                    -> Result<PathResolution, bool /* true if an error was reported */ > {
        let span = path.span;
        let segments = &path.segments[..path.segments.len() - path_depth];

        let mk_res = |def| PathResolution::new(def, path_depth);

        if path.global {
            let def = self.resolve_crate_relative_path(span, segments, namespace);
            return def.map(mk_res);
        }

        // Try to find a path to an item in a module.
        let last_ident = segments.last().unwrap().identifier;
        // Resolve a single identifier with fallback to primitive types
        let resolve_identifier_with_fallback = |this: &mut Self, record_used| {
            let def = this.resolve_identifier(last_ident, namespace, record_used);
            match def {
                None | Some(LocalDef{def: Def::Mod(..), ..}) if namespace == TypeNS =>
                    this.primitive_type_table
                        .primitive_types
                        .get(&last_ident.unhygienic_name)
                        .map_or(def, |prim_ty| Some(LocalDef::from_def(Def::PrimTy(*prim_ty)))),
                _ => def
            }
        };

        if segments.len() == 1 {
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
            let def = resolve_identifier_with_fallback(self, true).ok_or(false);
            return def.and_then(|def| self.adjust_local_def(def, span).ok_or(true)).map(mk_res);
        }

        let unqualified_def = resolve_identifier_with_fallback(self, false);
        let def = self.resolve_module_relative_path(span, segments, namespace);
        match (def, unqualified_def) {
            (Ok(d), Some(ref ud)) if d == ud.def => {
                self.session
                    .add_lint(lint::builtin::UNUSED_QUALIFICATIONS,
                              id,
                              span,
                              "unnecessary qualification".to_string());
            }
            _ => {}
        }

        def.map(mk_res)
    }

    // Resolve a single identifier
    fn resolve_identifier(&mut self,
                          identifier: hir::Ident,
                          namespace: Namespace,
                          record_used: bool)
                          -> Option<LocalDef> {
        if identifier.name == special_idents::invalid.name {
            return Some(LocalDef::from_def(Def::Err));
        }

        self.resolve_ident_in_lexical_scope(identifier, namespace, record_used)
            .map(LexicalScopeBinding::local_def)
    }

    // Resolve a local definition, potentially adjusting for closures.
    fn adjust_local_def(&mut self, local_def: LocalDef, span: Span) -> Option<Def> {
        let ribs = match local_def.ribs {
            Some((TypeNS, i)) => &self.type_ribs[i + 1..],
            Some((ValueNS, i)) => &self.value_ribs[i + 1..],
            _ => &[] as &[_],
        };
        let mut def = local_def.def;
        match def {
            Def::Upvar(..) => {
                span_bug!(span, "unexpected {:?} in bindings", def)
            }
            Def::Local(_, node_id) => {
                for rib in ribs {
                    match rib.kind {
                        NormalRibKind | ModuleRibKind(..) => {
                            // Nothing to do. Continue.
                        }
                        ClosureRibKind(function_id) => {
                            let prev_def = def;
                            let node_def_id = self.ast_map.local_def_id(node_id);

                            let seen = self.freevars_seen
                                           .entry(function_id)
                                           .or_insert_with(|| NodeMap());
                            if let Some(&index) = seen.get(&node_id) {
                                def = Def::Upvar(node_def_id, node_id, index, function_id);
                                continue;
                            }
                            let vec = self.freevars
                                          .entry(function_id)
                                          .or_insert_with(|| vec![]);
                            let depth = vec.len();
                            vec.push(Freevar {
                                def: prev_def,
                                span: span,
                            });

                            def = Def::Upvar(node_def_id, node_id, depth, function_id);
                            seen.insert(node_id, depth);
                        }
                        ItemRibKind | MethodRibKind => {
                            // This was an attempt to access an upvar inside a
                            // named function item. This is not allowed, so we
                            // report an error.
                            resolve_error(self,
                                          span,
                                          ResolutionError::CannotCaptureDynamicEnvironmentInFnItem);
                            return None;
                        }
                        ConstantItemRibKind => {
                            // Still doesn't deal with upvars
                            resolve_error(self,
                                          span,
                                          ResolutionError::AttemptToUseNonConstantValueInConstant);
                            return None;
                        }
                    }
                }
            }
            Def::TyParam(..) | Def::SelfTy(..) => {
                for rib in ribs {
                    match rib.kind {
                        NormalRibKind | MethodRibKind | ClosureRibKind(..) |
                        ModuleRibKind(..) => {
                            // Nothing to do. Continue.
                        }
                        ItemRibKind => {
                            // This was an attempt to use a type parameter outside
                            // its scope.

                            resolve_error(self,
                                          span,
                                          ResolutionError::TypeParametersFromOuterFunction);
                            return None;
                        }
                        ConstantItemRibKind => {
                            // see #9186
                            resolve_error(self, span, ResolutionError::OuterTypeParameterContext);
                            return None;
                        }
                    }
                }
            }
            _ => {}
        }
        return Some(def);
    }

    // resolve a "module-relative" path, e.g. a::b::c
    fn resolve_module_relative_path(&mut self,
                                    span: Span,
                                    segments: &[hir::PathSegment],
                                    namespace: Namespace)
                                    -> Result<Def, bool /* true if an error was reported */> {
        let module_path = segments.split_last()
                                  .unwrap()
                                  .1
                                  .iter()
                                  .map(|ps| ps.identifier.name)
                                  .collect::<Vec<_>>();

        let containing_module;
        match self.resolve_module_path(&module_path, UseLexicalScope, span) {
            Failed(err) => {
                let (span, msg) = match err {
                    Some((span, msg)) => (span, msg),
                    None => {
                        let msg = format!("Use of undeclared type or module `{}`",
                                          names_to_string(&module_path));
                        (span, msg)
                    }
                };

                resolve_error(self, span, ResolutionError::FailedToResolve(&msg));
                return Err(true);
            }
            Indeterminate => return Err(false),
            Success(resulting_module) => {
                containing_module = resulting_module;
            }
        }

        let name = segments.last().unwrap().identifier.name;
        let result = self.resolve_name_in_module(containing_module, name, namespace, false, true);
        result.success().map(|binding| {
            self.check_privacy(name, binding, span);
            binding.def().unwrap()
        }).ok_or(false)
    }

    /// Invariant: This must be called only during main resolution, not during
    /// import resolution.
    fn resolve_crate_relative_path(&mut self,
                                   span: Span,
                                   segments: &[hir::PathSegment],
                                   namespace: Namespace)
                                   -> Result<Def, bool /* true if an error was reported */> {
        let module_path = segments.split_last()
                                  .unwrap()
                                  .1
                                  .iter()
                                  .map(|ps| ps.identifier.name)
                                  .collect::<Vec<_>>();

        let root_module = self.graph_root;

        let containing_module;
        match self.resolve_module_path_from_root(root_module,
                                                 &module_path,
                                                 0,
                                                 span) {
            Failed(err) => {
                let (span, msg) = match err {
                    Some((span, msg)) => (span, msg),
                    None => {
                        let msg = format!("Use of undeclared module `::{}`",
                                          names_to_string(&module_path));
                        (span, msg)
                    }
                };

                resolve_error(self, span, ResolutionError::FailedToResolve(&msg));
                return Err(true);
            }

            Indeterminate => return Err(false),

            Success(resulting_module) => {
                containing_module = resulting_module;
            }
        }

        let name = segments.last().unwrap().identifier.name;
        let result = self.resolve_name_in_module(containing_module, name, namespace, false, true);
        result.success().map(|binding| {
            self.check_privacy(name, binding, span);
            binding.def().unwrap()
        }).ok_or(false)
    }

    fn with_no_errors<T, F>(&mut self, f: F) -> T
        where F: FnOnce(&mut Resolver) -> T
    {
        self.emit_errors = false;
        let rs = f(self);
        self.emit_errors = true;
        rs
    }

    fn find_fallback_in_self_type(&mut self, name: Name) -> FallbackSuggestion {
        fn extract_path_and_node_id(t: &Ty,
                                    allow: FallbackChecks)
                                    -> Option<(Path, NodeId, FallbackChecks)> {
            match t.node {
                TyPath(None, ref path) => Some((path.clone(), t.id, allow)),
                TyPtr(ref mut_ty) => extract_path_and_node_id(&mut_ty.ty, OnlyTraitAndStatics),
                TyRptr(_, ref mut_ty) => extract_path_and_node_id(&mut_ty.ty, allow),
                // This doesn't handle the remaining `Ty` variants as they are not
                // that commonly the self_type, it might be interesting to provide
                // support for those in future.
                _ => None,
            }
        }

        fn get_module<'a, 'tcx>(this: &mut Resolver<'a, 'tcx>,
                                span: Span,
                                name_path: &[ast::Name])
                                -> Option<Module<'a>> {
            let last_name = name_path.last().unwrap();

            if name_path.len() == 1 {
                match this.primitive_type_table.primitive_types.get(last_name) {
                    Some(_) => None,
                    None => this.current_module.resolve_name_in_lexical_scope(*last_name, TypeNS)
                                               .and_then(NameBinding::module)
                }
            } else {
                this.resolve_module_path(&name_path, UseLexicalScope, span).success()
            }
        }

        fn is_static_method(this: &Resolver, did: DefId) -> bool {
            if let Some(node_id) = this.ast_map.as_local_node_id(did) {
                let sig = match this.ast_map.get(node_id) {
                    hir_map::NodeTraitItem(trait_item) => match trait_item.node {
                        hir::MethodTraitItem(ref sig, _) => sig,
                        _ => return false,
                    },
                    hir_map::NodeImplItem(impl_item) => match impl_item.node {
                        hir::ImplItemKind::Method(ref sig, _) => sig,
                        _ => return false,
                    },
                    _ => return false,
                };
                sig.explicit_self.node == hir::SelfStatic
            } else {
                this.session.cstore.is_static_method(did)
            }
        }

        let (path, node_id, allowed) = match self.current_self_type {
            Some(ref ty) => match extract_path_and_node_id(ty, Everything) {
                Some(x) => x,
                None => return NoSuggestion,
            },
            None => return NoSuggestion,
        };

        if allowed == Everything {
            // Look for a field with the same name in the current self_type.
            match self.def_map.borrow().get(&node_id).map(|d| d.full_def()) {
                Some(Def::Enum(did)) |
                Some(Def::TyAlias(did)) |
                Some(Def::Struct(did)) |
                Some(Def::Variant(_, did)) => match self.structs.get(&did) {
                    None => {}
                    Some(fields) => {
                        if fields.iter().any(|&field_name| name == field_name) {
                            return Field;
                        }
                    }
                },
                _ => {} // Self type didn't resolve properly
            }
        }

        let name_path = path.segments.iter().map(|seg| seg.identifier.name).collect::<Vec<_>>();

        // Look for a method in the current self type's impl module.
        if let Some(module) = get_module(self, path.span, &name_path) {
            if let Some(binding) = module.resolve_name_in_lexical_scope(name, ValueNS) {
                if let Some(Def::Method(did)) = binding.def() {
                    if is_static_method(self, did) {
                        return StaticMethod(path_names_to_string(&path, 0));
                    }
                    if self.current_trait_ref.is_some() {
                        return TraitItem;
                    } else if allowed == Everything {
                        return Method;
                    }
                }
            }
        }

        // Look for a method in the current trait.
        if let Some((trait_did, ref trait_ref)) = self.current_trait_ref {
            if let Some(&did) = self.trait_item_map.get(&(name, trait_did)) {
                if is_static_method(self, did) {
                    return TraitMethod(path_names_to_string(&trait_ref.path, 0));
                } else {
                    return TraitItem;
                }
            }
        }

        NoSuggestion
    }

    fn find_best_match(&mut self, name: &str) -> SuggestionType {
        if let Some(macro_name) = self.session.available_macros
                                  .borrow().iter().find(|n| n.as_str() == name) {
            return SuggestionType::Macro(format!("{}!", macro_name));
        }

        let names = self.value_ribs
                    .iter()
                    .rev()
                    .flat_map(|rib| rib.bindings.keys());

        if let Some(found) = find_best_match_for_name(names, name, None) {
            if name != found {
                return SuggestionType::Function(found);
            }
        } SuggestionType::NotFound
    }

    fn resolve_expr(&mut self, expr: &Expr) {
        // First, record candidate traits for this expression if it could
        // result in the invocation of a method call.

        self.record_candidate_traits_for_expr_if_necessary(expr);

        // Next, resolve the node.
        match expr.node {
            ExprPath(ref maybe_qself, ref path) => {
                let resolution = match self.resolve_possibly_assoc_item(expr.id,
                                                                        maybe_qself.as_ref(),
                                                                        path,
                                                                        ValueNS) {
                    // `<T>::a::b::c` is resolved by typeck alone.
                    TypecheckRequired => {
                        let method_name = path.segments.last().unwrap().identifier.name;
                        let traits = self.get_traits_containing_item(method_name);
                        self.trait_map.insert(expr.id, traits);
                        intravisit::walk_expr(self, expr);
                        return;
                    }
                    ResolveAttempt(resolution) => resolution,
                };

                // This is a local path in the value namespace. Walk through
                // scopes looking for it.
                if let Some(path_res) = resolution {
                    // Check if struct variant
                    let is_struct_variant = if let Def::Variant(_, variant_id) = path_res.base_def {
                        self.structs.contains_key(&variant_id)
                    } else {
                        false
                    };
                    if is_struct_variant {
                        let _ = self.structs.contains_key(&path_res.base_def.def_id());
                        let path_name = path_names_to_string(path, 0);

                        let mut err = resolve_struct_error(self,
                                        expr.span,
                                        ResolutionError::StructVariantUsedAsFunction(&path_name));

                        let msg = format!("did you mean to write: `{} {{ /* fields */ }}`?",
                                          path_name);
                        if self.emit_errors {
                            err.fileline_help(expr.span, &msg);
                        } else {
                            err.span_help(expr.span, &msg);
                        }
                        err.emit();
                        self.record_def(expr.id, err_path_resolution());
                    } else {
                        // Write the result into the def map.
                        debug!("(resolving expr) resolved `{}`",
                               path_names_to_string(path, 0));

                        // Partial resolutions will need the set of traits in scope,
                        // so they can be completed during typeck.
                        if path_res.depth != 0 {
                            let method_name = path.segments.last().unwrap().identifier.name;
                            let traits = self.get_traits_containing_item(method_name);
                            self.trait_map.insert(expr.id, traits);
                        }

                        self.record_def(expr.id, path_res);
                    }
                } else {
                    // Be helpful if the name refers to a struct
                    // (The pattern matching def_tys where the id is in self.structs
                    // matches on regular structs while excluding tuple- and enum-like
                    // structs, which wouldn't result in this error.)
                    let path_name = path_names_to_string(path, 0);
                    let type_res = self.with_no_errors(|this| {
                        this.resolve_path(expr.id, path, 0, TypeNS)
                    });

                    self.record_def(expr.id, err_path_resolution());

                    if let Ok(Def::Struct(..)) = type_res.map(|r| r.base_def) {
                        let error_variant =
                            ResolutionError::StructVariantUsedAsFunction(&path_name);
                        let mut err = resolve_struct_error(self, expr.span, error_variant);

                        let msg = format!("did you mean to write: `{} {{ /* fields */ }}`?",
                                          path_name);

                        if self.emit_errors {
                            err.fileline_help(expr.span, &msg);
                        } else {
                            err.span_help(expr.span, &msg);
                        }
                        err.emit();
                    } else {
                        // Keep reporting some errors even if they're ignored above.
                        if let Err(true) = self.resolve_path(expr.id, path, 0, ValueNS) {
                            // `resolve_path` already reported the error
                        } else {
                            let mut method_scope = false;
                            self.value_ribs.iter().rev().all(|rib| {
                                method_scope = match rib.kind {
                                    MethodRibKind => true,
                                    ItemRibKind | ConstantItemRibKind => false,
                                    _ => return true, // Keep advancing
                                };
                                false // Stop advancing
                            });

                            if method_scope && special_names::self_.as_str() == &path_name[..] {
                                resolve_error(self,
                                              expr.span,
                                              ResolutionError::SelfNotAvailableInStaticMethod);
                            } else {
                                let last_name = path.segments.last().unwrap().identifier.name;
                                let mut msg = match self.find_fallback_in_self_type(last_name) {
                                    NoSuggestion => {
                                        // limit search to 5 to reduce the number
                                        // of stupid suggestions
                                        match self.find_best_match(&path_name) {
                                            SuggestionType::Macro(s) => {
                                                format!("the macro `{}`", s)
                                            }
                                            SuggestionType::Function(s) => format!("`{}`", s),
                                            SuggestionType::NotFound => "".to_string(),
                                        }
                                    }
                                    Field => format!("`self.{}`", path_name),
                                    Method |
                                    TraitItem => format!("to call `self.{}`", path_name),
                                    TraitMethod(path_str) |
                                    StaticMethod(path_str) =>
                                        format!("to call `{}::{}`", path_str, path_name),
                                };

                                let mut context =  UnresolvedNameContext::Other;
                                if !msg.is_empty() {
                                    msg = format!(". Did you mean {}?", msg);
                                } else {
                                    // we check if this a module and if so, we display a help
                                    // message
                                    let name_path = path.segments.iter()
                                                        .map(|seg| seg.identifier.name)
                                                        .collect::<Vec<_>>();

                                    match self.resolve_module_path(&name_path[..],
                                                                   UseLexicalScope,
                                                                   expr.span) {
                                        Success(_) => {
                                            context = UnresolvedNameContext::PathIsMod(expr.id);
                                        },
                                        _ => {},
                                    };
                                }

                                resolve_error(self,
                                              expr.span,
                                              ResolutionError::UnresolvedName(
                                                  &path_name, &msg, context));
                            }
                        }
                    }
                }

                intravisit::walk_expr(self, expr);
            }

            ExprStruct(ref path, _, _) => {
                // Resolve the path to the structure it goes to. We don't
                // check to ensure that the path is actually a structure; that
                // is checked later during typeck.
                match self.resolve_path(expr.id, path, 0, TypeNS) {
                    Ok(definition) => self.record_def(expr.id, definition),
                    Err(true) => self.record_def(expr.id, err_path_resolution()),
                    Err(false) => {
                        debug!("(resolving expression) didn't find struct def",);

                        resolve_error(self,
                                      path.span,
                                      ResolutionError::DoesNotNameAStruct(
                                                                &path_names_to_string(path, 0))
                                     );
                        self.record_def(expr.id, err_path_resolution());
                    }
                }

                intravisit::walk_expr(self, expr);
            }

            ExprLoop(_, Some(label)) | ExprWhile(_, _, Some(label)) => {
                self.with_label_rib(|this| {
                    let def = Def::Label(expr.id);

                    {
                        let rib = this.label_ribs.last_mut().unwrap();
                        rib.bindings.insert(label.name, def);
                    }

                    intravisit::walk_expr(this, expr);
                })
            }

            ExprBreak(Some(label)) | ExprAgain(Some(label)) => {
                match self.search_label(label.node.name) {
                    None => {
                        self.record_def(expr.id, err_path_resolution());
                        resolve_error(self,
                                      label.span,
                                      ResolutionError::UndeclaredLabel(&label.node.name.as_str()))
                    }
                    Some(def @ Def::Label(_)) => {
                        // Since this def is a label, it is never read.
                        self.record_def(expr.id,
                                        PathResolution {
                                            base_def: def,
                                            depth: 0,
                                        })
                    }
                    Some(_) => {
                        span_bug!(expr.span, "label wasn't mapped to a label def!")
                    }
                }
            }

            _ => {
                intravisit::walk_expr(self, expr);
            }
        }
    }

    fn record_candidate_traits_for_expr_if_necessary(&mut self, expr: &Expr) {
        match expr.node {
            ExprField(_, name) => {
                // FIXME(#6890): Even though you can't treat a method like a
                // field, we need to add any trait methods we find that match
                // the field name so that we can do some nice error reporting
                // later on in typeck.
                let traits = self.get_traits_containing_item(name.node);
                self.trait_map.insert(expr.id, traits);
            }
            ExprMethodCall(name, _, _) => {
                debug!("(recording candidate traits for expr) recording traits for {}",
                       expr.id);
                let traits = self.get_traits_containing_item(name.node);
                self.trait_map.insert(expr.id, traits);
            }
            _ => {
                // Nothing to do.
            }
        }
    }

    fn get_traits_containing_item(&mut self, name: Name) -> Vec<DefId> {
        debug!("(getting traits containing item) looking for '{}'", name);

        fn add_trait_info(found_traits: &mut Vec<DefId>, trait_def_id: DefId, name: Name) {
            debug!("(adding trait info) found trait {:?} for method '{}'",
                   trait_def_id,
                   name);
            found_traits.push(trait_def_id);
        }

        let mut found_traits = Vec::new();
        // Look for the current trait.
        if let Some((trait_def_id, _)) = self.current_trait_ref {
            if self.trait_item_map.contains_key(&(name, trait_def_id)) {
                add_trait_info(&mut found_traits, trait_def_id, name);
            }
        }

        let mut search_module = self.current_module;
        loop {
            // Look for trait children.
            let mut search_in_module = |module: Module<'a>| module.for_each_child(|_, ns, binding| {
                if ns != TypeNS { return }
                let trait_def_id = match binding.def() {
                    Some(Def::Trait(trait_def_id)) => trait_def_id,
                    Some(..) | None => return,
                };
                if self.trait_item_map.contains_key(&(name, trait_def_id)) {
                    add_trait_info(&mut found_traits, trait_def_id, name);
                    let trait_name = self.get_trait_name(trait_def_id);
                    self.record_use(trait_name, TypeNS, binding);
                }
            });
            search_in_module(search_module);

            match search_module.parent_link {
                NoParentLink | ModuleParentLink(..) => {
                    search_module.prelude.borrow().map(search_in_module);
                    break;
                }
                BlockParentLink(parent_module, _) => {
                    search_module = parent_module;
                }
            }
        }

        found_traits
    }

    /// When name resolution fails, this method can be used to look up candidate
    /// entities with the expected name. It allows filtering them using the
    /// supplied predicate (which should be used to only accept the types of
    /// definitions expected e.g. traits). The lookup spans across all crates.
    ///
    /// NOTE: The method does not look into imports, but this is not a problem,
    /// since we report the definitions (thus, the de-aliased imports).
    fn lookup_candidates<FilterFn>(&mut self,
                                   lookup_name: Name,
                                   namespace: Namespace,
                                   filter_fn: FilterFn) -> SuggestedCandidates
        where FilterFn: Fn(Def) -> bool {

        let mut lookup_results = Vec::new();
        let mut worklist = Vec::new();
        worklist.push((self.graph_root, Vec::new(), false));

        while let Some((in_module,
                        path_segments,
                        in_module_is_extern)) = worklist.pop() {
            self.populate_module_if_necessary(in_module);

            in_module.for_each_child(|name, ns, name_binding| {

                // avoid imports entirely
                if name_binding.is_import() { return; }

                // collect results based on the filter function
                if let Some(def) = name_binding.def() {
                    if name == lookup_name && ns == namespace && filter_fn(def) {
                        // create the path
                        let ident = hir::Ident::from_name(name);
                        let params = PathParameters::none();
                        let segment = PathSegment {
                            identifier: ident,
                            parameters: params,
                        };
                        let span = name_binding.span.unwrap_or(syntax::codemap::DUMMY_SP);
                        let mut segms = path_segments.clone();
                        segms.push(segment);
                        let segms = HirVec::from_vec(segms);
                        let path = Path {
                            span: span,
                            global: true,
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
                            lookup_results.push(path);
                        }
                    }
                }

                // collect submodules to explore
                if let Some(module) = name_binding.module() {
                    // form the path
                    let path_segments = match module.parent_link {
                        NoParentLink => path_segments.clone(),
                        ModuleParentLink(_, name) => {
                            let mut paths = path_segments.clone();
                            let ident = hir::Ident::from_name(name);
                            let params = PathParameters::none();
                            let segm = PathSegment {
                                identifier: ident,
                                parameters: params,
                            };
                            paths.push(segm);
                            paths
                        }
                        _ => bug!(),
                    };

                    if !in_module_is_extern || name_binding.vis == ty::Visibility::Public {
                        // add the module to the lookup
                        let is_extern = in_module_is_extern || name_binding.is_extern_crate();
                        worklist.push((module, path_segments, is_extern));
                    }
                }
            })
        }

        SuggestedCandidates {
            name: lookup_name.as_str().to_string(),
            candidates: lookup_results,
        }
    }

    fn record_def(&mut self, node_id: NodeId, resolution: PathResolution) {
        debug!("(recording def) recording {:?} for {}", resolution, node_id);
        if let Some(prev_res) = self.def_map.borrow_mut().insert(node_id, resolution) {
            let span = self.ast_map.opt_span(node_id).unwrap_or(codemap::DUMMY_SP);
            span_bug!(span,
                      "path resolved multiple times ({:?} before, {:?} now)",
                      prev_res,
                      resolution);
        }
    }

    fn enforce_default_binding_mode(&mut self,
                                    pat: &Pat,
                                    pat_binding_mode: BindingMode,
                                    descr: &str) {
        match pat_binding_mode {
            BindByValue(_) => {}
            BindByRef(..) => {
                resolve_error(self,
                              pat.span,
                              ResolutionError::CannotUseRefBindingModeWith(descr));
            }
        }
    }

    fn resolve_visibility(&mut self, vis: &hir::Visibility) -> ty::Visibility {
        let (path, id) = match *vis {
            hir::Public => return ty::Visibility::Public,
            hir::Visibility::Crate => return ty::Visibility::Restricted(ast::CRATE_NODE_ID),
            hir::Visibility::Restricted { ref path, id } => (path, id),
            hir::Inherited => {
                let current_module =
                    self.get_nearest_normal_module_parent_or_self(self.current_module);
                let id = self.ast_map.as_local_node_id(current_module.def_id().unwrap()).unwrap();
                return ty::Visibility::Restricted(id);
            }
        };

        let segments: Vec<_> = path.segments.iter().map(|seg| seg.identifier.name).collect();
        let vis = match self.resolve_module_path(&segments, DontUseLexicalScope, path.span) {
            Success(module) => {
                let def = module.def.unwrap();
                let path_resolution = PathResolution { base_def: def, depth: 0 };
                self.def_map.borrow_mut().insert(id, path_resolution);
                ty::Visibility::Restricted(self.ast_map.as_local_node_id(def.def_id()).unwrap())
            }
            Failed(Some((span, msg))) => {
                self.session.span_err(span, &format!("failed to resolve module path. {}", msg));
                ty::Visibility::Public
            }
            _ => {
                self.session.span_err(path.span, "unresolved module path");
                ty::Visibility::Public
            }
        };
        if !self.is_accessible(vis) {
            let msg = format!("visibilities can only be restricted to ancestor modules");
            self.session.span_err(path.span, &msg);
        }
        vis
    }

    fn is_accessible(&self, vis: ty::Visibility) -> bool {
        let current_module = self.get_nearest_normal_module_parent_or_self(self.current_module);
        let node_id = self.ast_map.as_local_node_id(current_module.def_id().unwrap()).unwrap();
        vis.is_accessible_from(node_id, &self.ast_map)
    }

    fn check_privacy(&mut self, name: Name, binding: &'a NameBinding<'a>, span: Span) {
        if !self.is_accessible(binding.vis) {
            self.privacy_errors.push(PrivacyError(span, name, binding));
        }
    }

    fn report_privacy_errors(&self) {
        if self.privacy_errors.len() == 0 { return }
        let mut reported_spans = HashSet::new();
        for &PrivacyError(span, name, binding) in &self.privacy_errors {
            if !reported_spans.insert(span) { continue }
            if binding.is_extern_crate() {
                // Warn when using an inaccessible extern crate.
                let node_id = binding.module().unwrap().extern_crate_id.unwrap();
                let msg = format!("extern crate `{}` is private", name);
                self.session.add_lint(lint::builtin::INACCESSIBLE_EXTERN_CRATE, node_id, span, msg);
            } else {
                let def = binding.def().unwrap();
                self.session.span_err(span, &format!("{} `{}` is private", def.kind_name(), name));
            }
        }
    }

    fn report_conflict(&self,
                       parent: Module,
                       name: Name,
                       ns: Namespace,
                       binding: &NameBinding,
                       old_binding: &NameBinding) {
        // Error on the second of two conflicting names
        if old_binding.span.unwrap().lo > binding.span.unwrap().lo {
            return self.report_conflict(parent, name, ns, old_binding, binding);
        }

        let container = match parent.def {
            Some(Def::Mod(_)) => "module",
            Some(Def::Trait(_)) => "trait",
            None => "block",
            _ => "enum",
        };

        let (participle, noun) = match old_binding.is_import() || old_binding.is_extern_crate() {
            true => ("imported", "import"),
            false => ("defined", "definition"),
        };

        let span = binding.span.unwrap();
        let msg = {
            let kind = match (ns, old_binding.module()) {
                (ValueNS, _) => "a value",
                (TypeNS, Some(module)) if module.extern_crate_id.is_some() => "an extern crate",
                (TypeNS, Some(module)) if module.is_normal() => "a module",
                (TypeNS, Some(module)) if module.is_trait() => "a trait",
                (TypeNS, _) => "a type",
            };
            format!("{} named `{}` has already been {} in this {}",
                    kind, name, participle, container)
        };

        let mut err = match (old_binding.is_extern_crate(), binding.is_extern_crate()) {
            (true, true) => struct_span_err!(self.session, span, E0259, "{}", msg),
            (true, _) | (_, true) if binding.is_import() || old_binding.is_import() =>
                struct_span_err!(self.session, span, E0254, "{}", msg),
            (true, _) | (_, true) => struct_span_err!(self.session, span, E0260, "{}", msg),
            _ => match (old_binding.is_import(), binding.is_import()) {
                (false, false) => struct_span_err!(self.session, span, E0428, "{}", msg),
                (true, true) => struct_span_err!(self.session, span, E0252, "{}", msg),
                _ => struct_span_err!(self.session, span, E0255, "{}", msg),
            },
        };

        let span = old_binding.span.unwrap();
        if span != codemap::DUMMY_SP {
            err.span_note(span, &format!("previous {} of `{}` here", noun, name));
        }
        err.emit();
    }
}

fn names_to_string(names: &[Name]) -> String {
    let mut first = true;
    let mut result = String::new();
    for name in names {
        if first {
            first = false
        } else {
            result.push_str("::")
        }
        result.push_str(&name.as_str());
    }
    result
}

fn path_names_to_string(path: &Path, depth: usize) -> String {
    let names: Vec<ast::Name> = path.segments[..path.segments.len() - depth]
                                    .iter()
                                    .map(|seg| seg.identifier.name)
                                    .collect();
    names_to_string(&names[..])
}

/// When an entity with a given name is not available in scope, we search for
/// entities with that name in all crates. This method allows outputting the
/// results of this search in a programmer-friendly way
fn show_candidates(session: &mut DiagnosticBuilder,
                   span: syntax::codemap::Span,
                   candidates: &SuggestedCandidates) {

    let paths = &candidates.candidates;

    if paths.len() > 0 {
        // don't show more than MAX_CANDIDATES results, so
        // we're consistent with the trait suggestions
        const MAX_CANDIDATES: usize = 5;

        // we want consistent results across executions, but candidates are produced
        // by iterating through a hash map, so make sure they are ordered:
        let mut path_strings: Vec<_> = paths.into_iter()
                                            .map(|p| path_names_to_string(&p, 0))
                                            .collect();
        path_strings.sort();

        // behave differently based on how many candidates we have:
        if !paths.is_empty() {
            if paths.len() == 1 {
                session.fileline_help(
                    span,
                    &format!("you can import it into scope: `use {};`.",
                        &path_strings[0]),
                );
            } else {
                session.fileline_help(span, "you can import several candidates \
                    into scope (`use ...;`):");
                let count = path_strings.len() as isize - MAX_CANDIDATES as isize + 1;

                for (idx, path_string) in path_strings.iter().enumerate() {
                    if idx == MAX_CANDIDATES - 1 && count > 1 {
                        session.fileline_help(
                            span,
                            &format!("  and {} other candidates", count).to_string(),
                        );
                        break;
                    } else {
                        session.fileline_help(
                            span,
                            &format!("  `{}`", path_string).to_string(),
                        );
                    }
                }
            }
        }
    } else {
        // nothing found:
        session.fileline_help(
            span,
            &format!("no candidates by the name of `{}` found in your \
            project; maybe you misspelled the name or forgot to import \
            an external crate?", candidates.name.to_string()),
        );
    };
}

/// A somewhat inefficient routine to obtain the name of a module.
fn module_to_string(module: Module) -> String {
    let mut names = Vec::new();

    fn collect_mod(names: &mut Vec<ast::Name>, module: Module) {
        match module.parent_link {
            NoParentLink => {}
            ModuleParentLink(ref module, name) => {
                names.push(name);
                collect_mod(names, module);
            }
            BlockParentLink(ref module, _) => {
                // danger, shouldn't be ident?
                names.push(special_idents::opaque.name);
                collect_mod(names, module);
            }
        }
    }
    collect_mod(&mut names, module);

    if names.is_empty() {
        return "???".to_string();
    }
    names_to_string(&names.into_iter().rev().collect::<Vec<ast::Name>>())
}

fn err_path_resolution() -> PathResolution {
    PathResolution {
        base_def: Def::Err,
        depth: 0,
    }
}


pub struct CrateMap {
    pub def_map: RefCell<DefMap>,
    pub freevars: FreevarMap,
    pub export_map: ExportMap,
    pub trait_map: TraitMap,
    pub glob_map: Option<GlobMap>,
}

#[derive(PartialEq,Copy, Clone)]
pub enum MakeGlobMap {
    Yes,
    No,
}

/// Entry point to crate resolution.
pub fn resolve_crate<'a, 'tcx>(session: &'a Session,
                               ast_map: &'a hir_map::Map<'tcx>,
                               make_glob_map: MakeGlobMap)
                               -> CrateMap {
    // Currently, we ignore the name resolution data structures for
    // the purposes of dependency tracking. Instead we will run name
    // resolution and include its output in the hash of each item,
    // much like we do for macro expansion. In other words, the hash
    // reflects not just its contents but the results of name
    // resolution on those contents. Hopefully we'll push this back at
    // some point.
    let _task = ast_map.dep_graph.in_task(DepNode::Resolve);

    let krate = ast_map.krate();
    let arenas = Resolver::arenas();
    let mut resolver = create_resolver(session, ast_map, krate, make_glob_map, &arenas, None);

    resolver.resolve_crate(krate);

    check_unused::check_crate(&mut resolver, krate);
    resolver.report_privacy_errors();

    CrateMap {
        def_map: resolver.def_map,
        freevars: resolver.freevars,
        export_map: resolver.export_map,
        trait_map: resolver.trait_map,
        glob_map: if resolver.make_glob_map {
            Some(resolver.glob_map)
        } else {
            None
        },
    }
}

/// Builds a name resolution walker to be used within this module,
/// or used externally, with an optional callback function.
///
/// The callback takes a &mut bool which allows callbacks to end a
/// walk when set to true, passing through the rest of the walk, while
/// preserving the ribs + current module. This allows resolve_path
/// calls to be made with the correct scope info. The node in the
/// callback corresponds to the current node in the walk.
fn create_resolver<'a, 'tcx>(session: &'a Session,
                             ast_map: &'a hir_map::Map<'tcx>,
                             krate: &'a Crate,
                             make_glob_map: MakeGlobMap,
                             arenas: &'a ResolverArenas<'a>,
                             callback: Option<Box<Fn(hir_map::Node, &mut bool) -> bool>>)
                             -> Resolver<'a, 'tcx> {
    let mut resolver = Resolver::new(session, ast_map, make_glob_map, arenas);

    resolver.callback = callback;

    resolver.build_reduced_graph(krate);

    resolve_imports::resolve_imports(&mut resolver);

    resolver
}

__build_diagnostic_array! { librustc_resolve, DIAGNOSTICS }
