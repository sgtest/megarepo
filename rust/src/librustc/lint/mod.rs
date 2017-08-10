// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Lints, aka compiler warnings.
//!
//! A 'lint' check is a kind of miscellaneous constraint that a user _might_
//! want to enforce, but might reasonably want to permit as well, on a
//! module-by-module basis. They contrast with static constraints enforced by
//! other phases of the compiler, which are generally required to hold in order
//! to compile the program at all.
//!
//! Most lints can be written as `LintPass` instances. These run just before
//! translation to LLVM bytecode. The `LintPass`es built into rustc are defined
//! within `builtin.rs`, which has further comments on how to add such a lint.
//! rustc can also load user-defined lint plugins via the plugin mechanism.
//!
//! Some of rustc's lints are defined elsewhere in the compiler and work by
//! calling `add_lint()` on the overall `Session` object. This works when
//! it happens before the main lint pass, which emits the lints stored by
//! `add_lint()`. To emit lints after the main lint pass (from trans, for
//! example) requires more effort. See `emit_lint` and `GatherNodeLevels`
//! in `context.rs`.

pub use self::Level::*;
pub use self::LintSource::*;

use std::rc::Rc;

use errors::DiagnosticBuilder;
use hir::def_id::{CrateNum, LOCAL_CRATE};
use hir::intravisit::{self, FnKind};
use hir;
use session::Session;
use std::ascii::AsciiExt;
use std::hash;
use syntax::ast;
use syntax::codemap::MultiSpan;
use syntax::symbol::Symbol;
use syntax::visit as ast_visit;
use syntax_pos::Span;
use ty::TyCtxt;
use ty::maps::Providers;
use util::nodemap::NodeMap;

pub use lint::context::{LateContext, EarlyContext, LintContext, LintStore,
                        check_crate, check_ast_crate,
                        FutureIncompatibleInfo, BufferedEarlyLint};

/// Specification of a single lint.
#[derive(Copy, Clone, Debug)]
pub struct Lint {
    /// A string identifier for the lint.
    ///
    /// This identifies the lint in attributes and in command-line arguments.
    /// In those contexts it is always lowercase, but this field is compared
    /// in a way which is case-insensitive for ASCII characters. This allows
    /// `declare_lint!()` invocations to follow the convention of upper-case
    /// statics without repeating the name.
    ///
    /// The name is written with underscores, e.g. "unused_imports".
    /// On the command line, underscores become dashes.
    pub name: &'static str,

    /// Default level for the lint.
    pub default_level: Level,

    /// Description of the lint or the issue it detects.
    ///
    /// e.g. "imports that are never used"
    pub desc: &'static str,
}

impl Lint {
    /// Get the lint's name, with ASCII letters converted to lowercase.
    pub fn name_lower(&self) -> String {
        self.name.to_ascii_lowercase()
    }
}

/// Build a `Lint` initializer.
#[macro_export]
macro_rules! lint_initializer {
    ($name:ident, $level:ident, $desc:expr) => (
        ::rustc::lint::Lint {
            name: stringify!($name),
            default_level: ::rustc::lint::$level,
            desc: $desc,
        }
    )
}

/// Declare a static item of type `&'static Lint`.
#[macro_export]
macro_rules! declare_lint {
    (pub $name:ident, $level:ident, $desc:expr) => (
        pub static $name: &'static ::rustc::lint::Lint
            = &lint_initializer!($name, $level, $desc);
    );
    ($name:ident, $level:ident, $desc:expr) => (
        static $name: &'static ::rustc::lint::Lint
            = &lint_initializer!($name, $level, $desc);
    );
}

/// Declare a static `LintArray` and return it as an expression.
#[macro_export]
macro_rules! lint_array { ($( $lint:expr ),*) => (
    {
        static ARRAY: LintArray = &[ $( &$lint ),* ];
        ARRAY
    }
) }

pub type LintArray = &'static [&'static &'static Lint];

pub trait LintPass {
    /// Get descriptions of the lints this `LintPass` object can emit.
    ///
    /// NB: there is no enforcement that the object only emits lints it registered.
    /// And some `rustc` internal `LintPass`es register lints to be emitted by other
    /// parts of the compiler. If you want enforced access restrictions for your
    /// `Lint`, make it a private `static` item in its own module.
    fn get_lints(&self) -> LintArray;
}


/// Trait for types providing lint checks.
///
/// Each `check` method checks a single syntax node, and should not
/// invoke methods recursively (unlike `Visitor`). By default they
/// do nothing.
//
// FIXME: eliminate the duplication with `Visitor`. But this also
// contains a few lint-specific methods with no equivalent in `Visitor`.
pub trait LateLintPass<'a, 'tcx>: LintPass {
    fn check_body(&mut self, _: &LateContext, _: &'tcx hir::Body) { }
    fn check_body_post(&mut self, _: &LateContext, _: &'tcx hir::Body) { }
    fn check_name(&mut self, _: &LateContext, _: Span, _: ast::Name) { }
    fn check_crate(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::Crate) { }
    fn check_crate_post(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::Crate) { }
    fn check_mod(&mut self,
                 _: &LateContext<'a, 'tcx>,
                 _: &'tcx hir::Mod,
                 _: Span,
                 _: ast::NodeId) { }
    fn check_mod_post(&mut self,
                      _: &LateContext<'a, 'tcx>,
                      _: &'tcx hir::Mod,
                      _: Span,
                      _: ast::NodeId) { }
    fn check_foreign_item(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::ForeignItem) { }
    fn check_foreign_item_post(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::ForeignItem) { }
    fn check_item(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::Item) { }
    fn check_item_post(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::Item) { }
    fn check_local(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::Local) { }
    fn check_block(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::Block) { }
    fn check_block_post(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::Block) { }
    fn check_stmt(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::Stmt) { }
    fn check_arm(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::Arm) { }
    fn check_pat(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::Pat) { }
    fn check_decl(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::Decl) { }
    fn check_expr(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::Expr) { }
    fn check_expr_post(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::Expr) { }
    fn check_ty(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::Ty) { }
    fn check_generics(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::Generics) { }
    fn check_fn(&mut self,
                _: &LateContext<'a, 'tcx>,
                _: FnKind<'tcx>,
                _: &'tcx hir::FnDecl,
                _: &'tcx hir::Body,
                _: Span,
                _: ast::NodeId) { }
    fn check_fn_post(&mut self,
                     _: &LateContext<'a, 'tcx>,
                     _: FnKind<'tcx>,
                     _: &'tcx hir::FnDecl,
                     _: &'tcx hir::Body,
                     _: Span,
                     _: ast::NodeId) { }
    fn check_trait_item(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::TraitItem) { }
    fn check_trait_item_post(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::TraitItem) { }
    fn check_impl_item(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::ImplItem) { }
    fn check_impl_item_post(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::ImplItem) { }
    fn check_struct_def(&mut self,
                        _: &LateContext<'a, 'tcx>,
                        _: &'tcx hir::VariantData,
                        _: ast::Name,
                        _: &'tcx hir::Generics,
                        _: ast::NodeId) { }
    fn check_struct_def_post(&mut self,
                             _: &LateContext<'a, 'tcx>,
                             _: &'tcx hir::VariantData,
                             _: ast::Name,
                             _: &'tcx hir::Generics,
                             _: ast::NodeId) { }
    fn check_struct_field(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::StructField) { }
    fn check_variant(&mut self,
                     _: &LateContext<'a, 'tcx>,
                     _: &'tcx hir::Variant,
                     _: &'tcx hir::Generics) { }
    fn check_variant_post(&mut self,
                          _: &LateContext<'a, 'tcx>,
                          _: &'tcx hir::Variant,
                          _: &'tcx hir::Generics) { }
    fn check_lifetime(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::Lifetime) { }
    fn check_lifetime_def(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::LifetimeDef) { }
    fn check_path(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx hir::Path, _: ast::NodeId) { }
    fn check_attribute(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx ast::Attribute) { }

    /// Called when entering a syntax node that can have lint attributes such
    /// as `#[allow(...)]`. Called with *all* the attributes of that node.
    fn enter_lint_attrs(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx [ast::Attribute]) { }

    /// Counterpart to `enter_lint_attrs`.
    fn exit_lint_attrs(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx [ast::Attribute]) { }
}

pub trait EarlyLintPass: LintPass {
    fn check_ident(&mut self, _: &EarlyContext, _: Span, _: ast::Ident) { }
    fn check_crate(&mut self, _: &EarlyContext, _: &ast::Crate) { }
    fn check_crate_post(&mut self, _: &EarlyContext, _: &ast::Crate) { }
    fn check_mod(&mut self, _: &EarlyContext, _: &ast::Mod, _: Span, _: ast::NodeId) { }
    fn check_mod_post(&mut self, _: &EarlyContext, _: &ast::Mod, _: Span, _: ast::NodeId) { }
    fn check_foreign_item(&mut self, _: &EarlyContext, _: &ast::ForeignItem) { }
    fn check_foreign_item_post(&mut self, _: &EarlyContext, _: &ast::ForeignItem) { }
    fn check_item(&mut self, _: &EarlyContext, _: &ast::Item) { }
    fn check_item_post(&mut self, _: &EarlyContext, _: &ast::Item) { }
    fn check_local(&mut self, _: &EarlyContext, _: &ast::Local) { }
    fn check_block(&mut self, _: &EarlyContext, _: &ast::Block) { }
    fn check_block_post(&mut self, _: &EarlyContext, _: &ast::Block) { }
    fn check_stmt(&mut self, _: &EarlyContext, _: &ast::Stmt) { }
    fn check_arm(&mut self, _: &EarlyContext, _: &ast::Arm) { }
    fn check_pat(&mut self, _: &EarlyContext, _: &ast::Pat) { }
    fn check_expr(&mut self, _: &EarlyContext, _: &ast::Expr) { }
    fn check_expr_post(&mut self, _: &EarlyContext, _: &ast::Expr) { }
    fn check_ty(&mut self, _: &EarlyContext, _: &ast::Ty) { }
    fn check_generics(&mut self, _: &EarlyContext, _: &ast::Generics) { }
    fn check_fn(&mut self, _: &EarlyContext,
        _: ast_visit::FnKind, _: &ast::FnDecl, _: Span, _: ast::NodeId) { }
    fn check_fn_post(&mut self, _: &EarlyContext,
        _: ast_visit::FnKind, _: &ast::FnDecl, _: Span, _: ast::NodeId) { }
    fn check_trait_item(&mut self, _: &EarlyContext, _: &ast::TraitItem) { }
    fn check_trait_item_post(&mut self, _: &EarlyContext, _: &ast::TraitItem) { }
    fn check_impl_item(&mut self, _: &EarlyContext, _: &ast::ImplItem) { }
    fn check_impl_item_post(&mut self, _: &EarlyContext, _: &ast::ImplItem) { }
    fn check_struct_def(&mut self, _: &EarlyContext,
        _: &ast::VariantData, _: ast::Ident, _: &ast::Generics, _: ast::NodeId) { }
    fn check_struct_def_post(&mut self, _: &EarlyContext,
        _: &ast::VariantData, _: ast::Ident, _: &ast::Generics, _: ast::NodeId) { }
    fn check_struct_field(&mut self, _: &EarlyContext, _: &ast::StructField) { }
    fn check_variant(&mut self, _: &EarlyContext, _: &ast::Variant, _: &ast::Generics) { }
    fn check_variant_post(&mut self, _: &EarlyContext, _: &ast::Variant, _: &ast::Generics) { }
    fn check_lifetime(&mut self, _: &EarlyContext, _: &ast::Lifetime) { }
    fn check_lifetime_def(&mut self, _: &EarlyContext, _: &ast::LifetimeDef) { }
    fn check_path(&mut self, _: &EarlyContext, _: &ast::Path, _: ast::NodeId) { }
    fn check_path_list_item(&mut self, _: &EarlyContext, _: &ast::PathListItem) { }
    fn check_attribute(&mut self, _: &EarlyContext, _: &ast::Attribute) { }

    /// Called when entering a syntax node that can have lint attributes such
    /// as `#[allow(...)]`. Called with *all* the attributes of that node.
    fn enter_lint_attrs(&mut self, _: &EarlyContext, _: &[ast::Attribute]) { }

    /// Counterpart to `enter_lint_attrs`.
    fn exit_lint_attrs(&mut self, _: &EarlyContext, _: &[ast::Attribute]) { }
}

/// A lint pass boxed up as a trait object.
pub type EarlyLintPassObject = Box<EarlyLintPass + 'static>;
pub type LateLintPassObject = Box<for<'a, 'tcx> LateLintPass<'a, 'tcx> + 'static>;

/// Identifies a lint known to the compiler.
#[derive(Clone, Copy, Debug)]
pub struct LintId {
    // Identity is based on pointer equality of this field.
    lint: &'static Lint,
}

impl PartialEq for LintId {
    fn eq(&self, other: &LintId) -> bool {
        (self.lint as *const Lint) == (other.lint as *const Lint)
    }
}

impl Eq for LintId { }

impl hash::Hash for LintId {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        let ptr = self.lint as *const Lint;
        ptr.hash(state);
    }
}

impl LintId {
    /// Get the `LintId` for a `Lint`.
    pub fn of(lint: &'static Lint) -> LintId {
        LintId {
            lint,
        }
    }

    /// Get the name of the lint.
    pub fn to_string(&self) -> String {
        self.lint.name_lower()
    }
}

/// Setting for how to handle a lint.
#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Debug, Hash)]
pub enum Level {
    Allow, Warn, Deny, Forbid
}

impl Level {
    /// Convert a level to a lower-case string.
    pub fn as_str(self) -> &'static str {
        match self {
            Allow => "allow",
            Warn => "warn",
            Deny => "deny",
            Forbid => "forbid",
        }
    }

    /// Convert a lower-case string to a level.
    pub fn from_str(x: &str) -> Option<Level> {
        match x {
            "allow" => Some(Allow),
            "warn" => Some(Warn),
            "deny" => Some(Deny),
            "forbid" => Some(Forbid),
            _ => None,
        }
    }
}

/// How a lint level was set.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LintSource {
    /// Lint is at the default level as declared
    /// in rustc or a plugin.
    Default,

    /// Lint level was set by an attribute.
    Node(ast::Name, Span),

    /// Lint level was set by a command-line flag.
    CommandLine(Symbol),
}

pub type LevelSource = (Level, LintSource);

pub mod builtin;
mod context;
mod levels;

pub use self::levels::{LintLevelSets, LintLevelMap};

pub struct LintBuffer {
    map: NodeMap<Vec<BufferedEarlyLint>>,
}

impl LintBuffer {
    pub fn new() -> LintBuffer {
        LintBuffer { map: NodeMap() }
    }

    pub fn add_lint(&mut self,
                    lint: &'static Lint,
                    id: ast::NodeId,
                    sp: MultiSpan,
                    msg: &str) {
        let early_lint = BufferedEarlyLint {
            lint_id: LintId::of(lint),
            ast_id: id,
            span: sp,
            msg: msg.to_string(),
        };
        let arr = self.map.entry(id).or_insert(Vec::new());
        if !arr.contains(&early_lint) {
            arr.push(early_lint);
        }
    }

    pub fn take(&mut self, id: ast::NodeId) -> Vec<BufferedEarlyLint> {
        self.map.remove(&id).unwrap_or(Vec::new())
    }

    pub fn get_any(&self) -> Option<&[BufferedEarlyLint]> {
        let key = self.map.keys().next().map(|k| *k);
        key.map(|k| &self.map[&k][..])
    }
}

pub fn struct_lint_level<'a>(sess: &'a Session,
                             lint: &'static Lint,
                             level: Level,
                             src: LintSource,
                             span: Option<MultiSpan>,
                             msg: &str)
    -> DiagnosticBuilder<'a>
{
    let mut err = match (level, span) {
        (Level::Allow, _) => return sess.diagnostic().struct_dummy(),
        (Level::Warn, Some(span)) => sess.struct_span_warn(span, msg),
        (Level::Warn, None) => sess.struct_warn(msg),
        (Level::Deny, Some(span)) |
        (Level::Forbid, Some(span)) => sess.struct_span_err(span, msg),
        (Level::Deny, None) |
        (Level::Forbid, None) => sess.struct_err(msg),
    };

    let name = lint.name_lower();
    match src {
        LintSource::Default => {
            sess.diag_note_once(
                &mut err,
                lint,
                &format!("#[{}({})] on by default", level.as_str(), name));
        }
        LintSource::CommandLine(lint_flag_val) => {
            let flag = match level {
                Level::Warn => "-W",
                Level::Deny => "-D",
                Level::Forbid => "-F",
                Level::Allow => panic!(),
            };
            let hyphen_case_lint_name = name.replace("_", "-");
            if lint_flag_val.as_str() == name {
                sess.diag_note_once(
                    &mut err,
                    lint,
                    &format!("requested on the command line with `{} {}`",
                             flag, hyphen_case_lint_name));
            } else {
                let hyphen_case_flag_val = lint_flag_val.as_str().replace("_", "-");
                sess.diag_note_once(
                    &mut err,
                    lint,
                    &format!("`{} {}` implied by `{} {}`",
                             flag, hyphen_case_lint_name, flag,
                             hyphen_case_flag_val));
            }
        }
        LintSource::Node(lint_attr_name, src) => {
            sess.diag_span_note_once(&mut err, lint, src, "lint level defined here");
            if lint_attr_name.as_str() != name {
                let level_str = level.as_str();
                sess.diag_note_once(&mut err, lint,
                                    &format!("#[{}({})] implied by #[{}({})]",
                                             level_str, name, level_str, lint_attr_name));
            }
        }
    }

    // Check for future incompatibility lints and issue a stronger warning.
    let lints = sess.lint_store.borrow();
    if let Some(future_incompatible) = lints.future_incompatible(LintId::of(lint)) {
        let explanation = format!("this was previously accepted by the compiler \
                                   but is being phased out; \
                                   it will become a hard error in a future release!");
        let citation = format!("for more information, see {}",
                               future_incompatible.reference);
        err.warn(&explanation);
        err.note(&citation);
    }

    return err
}

fn lint_levels<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>, cnum: CrateNum)
    -> Rc<LintLevelMap>
{
    assert_eq!(cnum, LOCAL_CRATE);
    let mut builder = LintLevelMapBuilder {
        levels: LintLevelSets::builder(tcx.sess),
        tcx: tcx,
    };
    let krate = tcx.hir.krate();

    builder.with_lint_attrs(ast::CRATE_NODE_ID, &krate.attrs, |builder| {
        intravisit::walk_crate(builder, krate);
    });

    Rc::new(builder.levels.build_map())
}

struct LintLevelMapBuilder<'a, 'tcx: 'a> {
    levels: levels::LintLevelsBuilder<'tcx>,
    tcx: TyCtxt<'a, 'tcx, 'tcx>,
}

impl<'a, 'tcx> LintLevelMapBuilder<'a, 'tcx> {
    fn with_lint_attrs<F>(&mut self,
                          id: ast::NodeId,
                          attrs: &[ast::Attribute],
                          f: F)
        where F: FnOnce(&mut Self)
    {
        let push = self.levels.push(attrs);
        self.levels.register_id(self.tcx.hir.definitions().node_to_hir_id(id));
        f(self);
        self.levels.pop(push);
    }
}

impl<'a, 'tcx> intravisit::Visitor<'tcx> for LintLevelMapBuilder<'a, 'tcx> {
    fn nested_visit_map<'this>(&'this mut self) -> intravisit::NestedVisitorMap<'this, 'tcx> {
        intravisit::NestedVisitorMap::All(&self.tcx.hir)
    }

    fn visit_item(&mut self, it: &'tcx hir::Item) {
        self.with_lint_attrs(it.id, &it.attrs, |builder| {
            intravisit::walk_item(builder, it);
        });
    }

    fn visit_foreign_item(&mut self, it: &'tcx hir::ForeignItem) {
        self.with_lint_attrs(it.id, &it.attrs, |builder| {
            intravisit::walk_foreign_item(builder, it);
        })
    }

    fn visit_expr(&mut self, e: &'tcx hir::Expr) {
        self.with_lint_attrs(e.id, &e.attrs, |builder| {
            intravisit::walk_expr(builder, e);
        })
    }

    fn visit_struct_field(&mut self, s: &'tcx hir::StructField) {
        self.with_lint_attrs(s.id, &s.attrs, |builder| {
            intravisit::walk_struct_field(builder, s);
        })
    }

    fn visit_variant(&mut self,
                     v: &'tcx hir::Variant,
                     g: &'tcx hir::Generics,
                     item_id: ast::NodeId) {
        self.with_lint_attrs(v.node.data.id(), &v.node.attrs, |builder| {
            intravisit::walk_variant(builder, v, g, item_id);
        })
    }

    fn visit_local(&mut self, l: &'tcx hir::Local) {
        self.with_lint_attrs(l.id, &l.attrs, |builder| {
            intravisit::walk_local(builder, l);
        })
    }

    fn visit_trait_item(&mut self, trait_item: &'tcx hir::TraitItem) {
        self.with_lint_attrs(trait_item.id, &trait_item.attrs, |builder| {
            intravisit::walk_trait_item(builder, trait_item);
        });
    }

    fn visit_impl_item(&mut self, impl_item: &'tcx hir::ImplItem) {
        self.with_lint_attrs(impl_item.id, &impl_item.attrs, |builder| {
            intravisit::walk_impl_item(builder, impl_item);
        });
    }
}

pub fn provide(providers: &mut Providers) {
    providers.lint_levels = lint_levels;
}
