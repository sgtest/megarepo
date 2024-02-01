//! `hir_expand` deals with macro expansion.
//!
//! Specifically, it implements a concept of `MacroFile` -- a file whose syntax
//! tree originates not from the text of some `FileId`, but from some macro
//! expansion.

#![warn(rust_2018_idioms, unused_lifetimes)]

pub mod ast_id_map;
pub mod attrs;
pub mod builtin_attr_macro;
pub mod builtin_derive_macro;
pub mod builtin_fn_macro;
pub mod change;
pub mod db;
pub mod declarative;
pub mod eager;
pub mod files;
pub mod hygiene;
pub mod mod_path;
pub mod name;
pub mod proc_macro;
pub mod quote;
pub mod span_map;

mod fixup;

use attrs::collect_attrs;
use triomphe::Arc;

use std::{fmt, hash::Hash};

use base_db::{CrateId, Edition, FileId};
use either::Either;
use span::{FileRange, HirFileIdRepr, Span, SyntaxContextId};
use syntax::{
    ast::{self, AstNode},
    SyntaxNode, SyntaxToken, TextRange, TextSize,
};

use crate::{
    attrs::AttrId,
    builtin_attr_macro::BuiltinAttrExpander,
    builtin_derive_macro::BuiltinDeriveExpander,
    builtin_fn_macro::{BuiltinFnLikeExpander, EagerExpander},
    db::{ExpandDatabase, TokenExpander},
    fixup::SyntaxFixupUndoInfo,
    hygiene::SyntaxContextData,
    mod_path::ModPath,
    proc_macro::{CustomProcMacroExpander, ProcMacroKind},
    span_map::{ExpansionSpanMap, SpanMap},
};

pub use crate::ast_id_map::{AstId, ErasedAstId, ErasedFileAstId};
pub use crate::files::{InFile, InMacroFile, InRealFile};

pub use mbe::ValueResult;
pub use span::{HirFileId, MacroCallId, MacroFileId};

pub type DeclarativeMacro = ::mbe::DeclarativeMacro<tt::Span>;

pub mod tt {
    pub use span::Span;
    pub use tt::{DelimiterKind, Spacing};

    pub type Delimiter = ::tt::Delimiter<Span>;
    pub type DelimSpan = ::tt::DelimSpan<Span>;
    pub type Subtree = ::tt::Subtree<Span>;
    pub type Leaf = ::tt::Leaf<Span>;
    pub type Literal = ::tt::Literal<Span>;
    pub type Punct = ::tt::Punct<Span>;
    pub type Ident = ::tt::Ident<Span>;
    pub type TokenTree = ::tt::TokenTree<Span>;
}

#[macro_export]
macro_rules! impl_intern_lookup {
    ($db:ident, $id:ident, $loc:ident, $intern:ident, $lookup:ident) => {
        impl $crate::Intern for $loc {
            type Database<'db> = dyn $db + 'db;
            type ID = $id;
            fn intern(self, db: &Self::Database<'_>) -> $id {
                db.$intern(self)
            }
        }

        impl $crate::Lookup for $id {
            type Database<'db> = dyn $db + 'db;
            type Data = $loc;
            fn lookup(&self, db: &Self::Database<'_>) -> $loc {
                db.$lookup(*self)
            }
        }
    };
}

// ideally these would be defined in base-db, but the orphan rule doesn't let us
pub trait Intern {
    type Database<'db>: ?Sized;
    type ID;
    fn intern(self, db: &Self::Database<'_>) -> Self::ID;
}

pub trait Lookup {
    type Database<'db>: ?Sized;
    type Data;
    fn lookup(&self, db: &Self::Database<'_>) -> Self::Data;
}

impl_intern_lookup!(
    ExpandDatabase,
    MacroCallId,
    MacroCallLoc,
    intern_macro_call,
    lookup_intern_macro_call
);

impl_intern_lookup!(
    ExpandDatabase,
    SyntaxContextId,
    SyntaxContextData,
    intern_syntax_context,
    lookup_intern_syntax_context
);

pub type ExpandResult<T> = ValueResult<T, ExpandError>;

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum ExpandError {
    UnresolvedProcMacro(CrateId),
    Mbe(mbe::ExpandError),
    RecursionOverflowPoisoned,
    Other(Box<Box<str>>),
    ProcMacroPanic(Box<Box<str>>),
}

impl ExpandError {
    pub fn other(msg: impl Into<Box<str>>) -> Self {
        ExpandError::Other(Box::new(msg.into()))
    }
}

impl From<mbe::ExpandError> for ExpandError {
    fn from(mbe: mbe::ExpandError) -> Self {
        Self::Mbe(mbe)
    }
}

impl fmt::Display for ExpandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExpandError::UnresolvedProcMacro(_) => f.write_str("unresolved proc-macro"),
            ExpandError::Mbe(it) => it.fmt(f),
            ExpandError::RecursionOverflowPoisoned => {
                f.write_str("overflow expanding the original macro")
            }
            ExpandError::ProcMacroPanic(it) => {
                f.write_str("proc-macro panicked: ")?;
                f.write_str(it)
            }
            ExpandError::Other(it) => f.write_str(it),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MacroCallLoc {
    pub def: MacroDefId,
    pub krate: CrateId,
    /// Some if this is a macro call for an eager macro. Note that this is `None`
    /// for the eager input macro file.
    // FIXME: This is being interned, subtrees can vary quickly differ just slightly causing
    // leakage problems here
    eager: Option<Arc<EagerCallInfo>>,
    pub kind: MacroCallKind,
    pub call_site: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MacroDefId {
    pub krate: CrateId,
    pub edition: Edition,
    pub kind: MacroDefKind,
    pub local_inner: bool,
    pub allow_internal_unsafe: bool,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacroDefKind {
    Declarative(AstId<ast::Macro>),
    BuiltIn(BuiltinFnLikeExpander, AstId<ast::Macro>),
    BuiltInAttr(BuiltinAttrExpander, AstId<ast::Macro>),
    BuiltInDerive(BuiltinDeriveExpander, AstId<ast::Macro>),
    BuiltInEager(EagerExpander, AstId<ast::Macro>),
    ProcMacro(CustomProcMacroExpander, ProcMacroKind, AstId<ast::Fn>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EagerCallInfo {
    /// The expanded argument of the eager macro.
    arg: Arc<tt::Subtree>,
    /// Call id of the eager macro's input file (this is the macro file for its fully expanded input).
    arg_id: MacroCallId,
    error: Option<ExpandError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MacroCallKind {
    FnLike {
        ast_id: AstId<ast::MacroCall>,
        expand_to: ExpandTo,
    },
    Derive {
        ast_id: AstId<ast::Adt>,
        /// Syntactical index of the invoking `#[derive]` attribute.
        ///
        /// Outer attributes are counted first, then inner attributes. This does not support
        /// out-of-line modules, which may have attributes spread across 2 files!
        derive_attr_index: AttrId,
        /// Index of the derive macro in the derive attribute
        derive_index: u32,
    },
    Attr {
        ast_id: AstId<ast::Item>,
        // FIXME: This is being interned, subtrees can vary quickly differ just slightly causing
        // leakage problems here
        attr_args: Option<Arc<tt::Subtree>>,
        /// Syntactical index of the invoking `#[attribute]`.
        ///
        /// Outer attributes are counted first, then inner attributes. This does not support
        /// out-of-line modules, which may have attributes spread across 2 files!
        invoc_attr_index: AttrId,
    },
}

pub trait HirFileIdExt {
    /// Returns the original file of this macro call hierarchy.
    fn original_file(self, db: &dyn ExpandDatabase) -> FileId;

    /// Returns the original file of this macro call hierarchy while going into the included file if
    /// one of the calls comes from an `include!``.
    fn original_file_respecting_includes(self, db: &dyn ExpandDatabase) -> FileId;

    /// If this is a macro call, returns the syntax node of the very first macro call this file resides in.
    fn original_call_node(self, db: &dyn ExpandDatabase) -> Option<InRealFile<SyntaxNode>>;

    /// Return expansion information if it is a macro-expansion file
    fn expansion_info(self, db: &dyn ExpandDatabase) -> Option<ExpansionInfo>;

    fn as_builtin_derive_attr_node(&self, db: &dyn ExpandDatabase) -> Option<InFile<ast::Attr>>;
}

impl HirFileIdExt for HirFileId {
    fn original_file(self, db: &dyn ExpandDatabase) -> FileId {
        let mut file_id = self;
        loop {
            match file_id.repr() {
                HirFileIdRepr::FileId(id) => break id,
                HirFileIdRepr::MacroFile(MacroFileId { macro_call_id }) => {
                    file_id = macro_call_id.lookup(db).kind.file_id();
                }
            }
        }
    }

    fn original_file_respecting_includes(mut self, db: &dyn ExpandDatabase) -> FileId {
        loop {
            match self.repr() {
                HirFileIdRepr::FileId(id) => break id,
                HirFileIdRepr::MacroFile(file) => {
                    let loc = db.lookup_intern_macro_call(file.macro_call_id);
                    if loc.def.is_include() {
                        if let Some(eager) = &loc.eager {
                            if let Ok(it) = builtin_fn_macro::include_input_to_file_id(
                                db,
                                file.macro_call_id,
                                &eager.arg,
                            ) {
                                break it;
                            }
                        }
                    }
                    self = loc.kind.file_id();
                }
            }
        }
    }

    fn original_call_node(self, db: &dyn ExpandDatabase) -> Option<InRealFile<SyntaxNode>> {
        let mut call = db.lookup_intern_macro_call(self.macro_file()?.macro_call_id).to_node(db);
        loop {
            match call.file_id.repr() {
                HirFileIdRepr::FileId(file_id) => {
                    break Some(InRealFile { file_id, value: call.value })
                }
                HirFileIdRepr::MacroFile(MacroFileId { macro_call_id }) => {
                    call = db.lookup_intern_macro_call(macro_call_id).to_node(db);
                }
            }
        }
    }

    /// Return expansion information if it is a macro-expansion file
    fn expansion_info(self, db: &dyn ExpandDatabase) -> Option<ExpansionInfo> {
        Some(ExpansionInfo::new(db, self.macro_file()?))
    }

    fn as_builtin_derive_attr_node(&self, db: &dyn ExpandDatabase) -> Option<InFile<ast::Attr>> {
        let macro_file = self.macro_file()?;
        let loc: MacroCallLoc = db.lookup_intern_macro_call(macro_file.macro_call_id);
        let attr = match loc.def.kind {
            MacroDefKind::BuiltInDerive(..) => loc.to_node(db),
            _ => return None,
        };
        Some(attr.with_value(ast::Attr::cast(attr.value.clone())?))
    }
}

pub trait MacroFileIdExt {
    fn expansion_level(self, db: &dyn ExpandDatabase) -> u32;
    /// If this is a macro call, returns the syntax node of the call.
    fn call_node(self, db: &dyn ExpandDatabase) -> InFile<SyntaxNode>;
    fn parent(self, db: &dyn ExpandDatabase) -> HirFileId;

    fn expansion_info(self, db: &dyn ExpandDatabase) -> ExpansionInfo;

    fn is_builtin_derive(&self, db: &dyn ExpandDatabase) -> bool;
    fn is_custom_derive(&self, db: &dyn ExpandDatabase) -> bool;

    /// Return whether this file is an include macro
    fn is_include_macro(&self, db: &dyn ExpandDatabase) -> bool;

    fn is_eager(&self, db: &dyn ExpandDatabase) -> bool;
    /// Return whether this file is an attr macro
    fn is_attr_macro(&self, db: &dyn ExpandDatabase) -> bool;

    /// Return whether this file is the pseudo expansion of the derive attribute.
    /// See [`crate::builtin_attr_macro::derive_attr_expand`].
    fn is_derive_attr_pseudo_expansion(&self, db: &dyn ExpandDatabase) -> bool;
}

impl MacroFileIdExt for MacroFileId {
    fn call_node(self, db: &dyn ExpandDatabase) -> InFile<SyntaxNode> {
        db.lookup_intern_macro_call(self.macro_call_id).to_node(db)
    }
    fn expansion_level(self, db: &dyn ExpandDatabase) -> u32 {
        let mut level = 0;
        let mut macro_file = self;
        loop {
            let loc: MacroCallLoc = db.lookup_intern_macro_call(macro_file.macro_call_id);

            level += 1;
            macro_file = match loc.kind.file_id().repr() {
                HirFileIdRepr::FileId(_) => break level,
                HirFileIdRepr::MacroFile(it) => it,
            };
        }
    }
    fn parent(self, db: &dyn ExpandDatabase) -> HirFileId {
        self.macro_call_id.lookup(db).kind.file_id()
    }

    /// Return expansion information if it is a macro-expansion file
    fn expansion_info(self, db: &dyn ExpandDatabase) -> ExpansionInfo {
        ExpansionInfo::new(db, self)
    }

    fn is_custom_derive(&self, db: &dyn ExpandDatabase) -> bool {
        matches!(
            db.lookup_intern_macro_call(self.macro_call_id).def.kind,
            MacroDefKind::ProcMacro(_, ProcMacroKind::CustomDerive, _)
        )
    }

    fn is_builtin_derive(&self, db: &dyn ExpandDatabase) -> bool {
        matches!(
            db.lookup_intern_macro_call(self.macro_call_id).def.kind,
            MacroDefKind::BuiltInDerive(..)
        )
    }

    fn is_include_macro(&self, db: &dyn ExpandDatabase) -> bool {
        db.lookup_intern_macro_call(self.macro_call_id).def.is_include()
    }

    fn is_eager(&self, db: &dyn ExpandDatabase) -> bool {
        let loc: MacroCallLoc = db.lookup_intern_macro_call(self.macro_call_id);
        matches!(loc.def.kind, MacroDefKind::BuiltInEager(..))
    }

    fn is_attr_macro(&self, db: &dyn ExpandDatabase) -> bool {
        let loc: MacroCallLoc = db.lookup_intern_macro_call(self.macro_call_id);
        matches!(loc.kind, MacroCallKind::Attr { .. })
    }

    fn is_derive_attr_pseudo_expansion(&self, db: &dyn ExpandDatabase) -> bool {
        let loc: MacroCallLoc = db.lookup_intern_macro_call(self.macro_call_id);
        loc.def.is_attribute_derive()
    }
}

impl MacroDefId {
    pub fn as_lazy_macro(
        self,
        db: &dyn ExpandDatabase,
        krate: CrateId,
        kind: MacroCallKind,
        call_site: Span,
    ) -> MacroCallId {
        MacroCallLoc { def: self, krate, eager: None, kind, call_site }.intern(db)
    }

    pub fn definition_range(&self, db: &dyn ExpandDatabase) -> InFile<TextRange> {
        match self.kind {
            MacroDefKind::Declarative(id)
            | MacroDefKind::BuiltIn(_, id)
            | MacroDefKind::BuiltInAttr(_, id)
            | MacroDefKind::BuiltInDerive(_, id)
            | MacroDefKind::BuiltInEager(_, id) => {
                id.with_value(db.ast_id_map(id.file_id).get(id.value).text_range())
            }
            MacroDefKind::ProcMacro(_, _, id) => {
                id.with_value(db.ast_id_map(id.file_id).get(id.value).text_range())
            }
        }
    }

    pub fn ast_id(&self) -> Either<AstId<ast::Macro>, AstId<ast::Fn>> {
        match self.kind {
            MacroDefKind::ProcMacro(.., id) => Either::Right(id),
            MacroDefKind::Declarative(id)
            | MacroDefKind::BuiltIn(_, id)
            | MacroDefKind::BuiltInAttr(_, id)
            | MacroDefKind::BuiltInDerive(_, id)
            | MacroDefKind::BuiltInEager(_, id) => Either::Left(id),
        }
    }

    pub fn is_proc_macro(&self) -> bool {
        matches!(self.kind, MacroDefKind::ProcMacro(..))
    }

    pub fn is_attribute(&self) -> bool {
        matches!(
            self.kind,
            MacroDefKind::BuiltInAttr(..) | MacroDefKind::ProcMacro(_, ProcMacroKind::Attr, _)
        )
    }

    pub fn is_derive(&self) -> bool {
        matches!(
            self.kind,
            MacroDefKind::BuiltInDerive(..)
                | MacroDefKind::ProcMacro(_, ProcMacroKind::CustomDerive, _)
        )
    }

    pub fn is_fn_like(&self) -> bool {
        matches!(
            self.kind,
            MacroDefKind::BuiltIn(..)
                | MacroDefKind::ProcMacro(_, ProcMacroKind::FuncLike, _)
                | MacroDefKind::BuiltInEager(..)
                | MacroDefKind::Declarative(..)
        )
    }

    pub fn is_attribute_derive(&self) -> bool {
        matches!(self.kind, MacroDefKind::BuiltInAttr(expander, ..) if expander.is_derive())
    }

    pub fn is_include(&self) -> bool {
        matches!(self.kind, MacroDefKind::BuiltInEager(expander, ..) if expander.is_include())
    }
}

impl MacroCallLoc {
    pub fn to_node(&self, db: &dyn ExpandDatabase) -> InFile<SyntaxNode> {
        match self.kind {
            MacroCallKind::FnLike { ast_id, .. } => {
                ast_id.with_value(ast_id.to_node(db).syntax().clone())
            }
            MacroCallKind::Derive { ast_id, derive_attr_index, .. } => {
                // FIXME: handle `cfg_attr`
                ast_id.with_value(ast_id.to_node(db)).map(|it| {
                    collect_attrs(&it)
                        .nth(derive_attr_index.ast_index())
                        .and_then(|it| match it.1 {
                            Either::Left(attr) => Some(attr.syntax().clone()),
                            Either::Right(_) => None,
                        })
                        .unwrap_or_else(|| it.syntax().clone())
                })
            }
            MacroCallKind::Attr { ast_id, invoc_attr_index, .. } => {
                if self.def.is_attribute_derive() {
                    // FIXME: handle `cfg_attr`
                    ast_id.with_value(ast_id.to_node(db)).map(|it| {
                        collect_attrs(&it)
                            .nth(invoc_attr_index.ast_index())
                            .and_then(|it| match it.1 {
                                Either::Left(attr) => Some(attr.syntax().clone()),
                                Either::Right(_) => None,
                            })
                            .unwrap_or_else(|| it.syntax().clone())
                    })
                } else {
                    ast_id.with_value(ast_id.to_node(db).syntax().clone())
                }
            }
        }
    }

    fn expand_to(&self) -> ExpandTo {
        match self.kind {
            MacroCallKind::FnLike { expand_to, .. } => expand_to,
            MacroCallKind::Derive { .. } => ExpandTo::Items,
            MacroCallKind::Attr { .. } if self.def.is_attribute_derive() => ExpandTo::Items,
            MacroCallKind::Attr { .. } => {
                // FIXME(stmt_expr_attributes)
                ExpandTo::Items
            }
        }
    }
}

impl MacroCallKind {
    fn descr(&self) -> &'static str {
        match self {
            MacroCallKind::FnLike { .. } => "macro call",
            MacroCallKind::Derive { .. } => "derive macro",
            MacroCallKind::Attr { .. } => "attribute macro",
        }
    }

    /// Returns the file containing the macro invocation.
    pub fn file_id(&self) -> HirFileId {
        match *self {
            MacroCallKind::FnLike { ast_id: InFile { file_id, .. }, .. }
            | MacroCallKind::Derive { ast_id: InFile { file_id, .. }, .. }
            | MacroCallKind::Attr { ast_id: InFile { file_id, .. }, .. } => file_id,
        }
    }

    pub fn erased_ast_id(&self) -> ErasedFileAstId {
        match *self {
            MacroCallKind::FnLike { ast_id: InFile { value, .. }, .. } => value.erase(),
            MacroCallKind::Derive { ast_id: InFile { value, .. }, .. } => value.erase(),
            MacroCallKind::Attr { ast_id: InFile { value, .. }, .. } => value.erase(),
        }
    }

    /// Returns the original file range that best describes the location of this macro call.
    ///
    /// Unlike `MacroCallKind::original_call_range`, this also spans the item of attributes and derives.
    pub fn original_call_range_with_body(self, db: &dyn ExpandDatabase) -> FileRange {
        let mut kind = self;
        let file_id = loop {
            match kind.file_id().repr() {
                HirFileIdRepr::MacroFile(file) => {
                    kind = db.lookup_intern_macro_call(file.macro_call_id).kind;
                }
                HirFileIdRepr::FileId(file_id) => break file_id,
            }
        };

        let range = match kind {
            MacroCallKind::FnLike { ast_id, .. } => ast_id.to_ptr(db).text_range(),
            MacroCallKind::Derive { ast_id, .. } => ast_id.to_ptr(db).text_range(),
            MacroCallKind::Attr { ast_id, .. } => ast_id.to_ptr(db).text_range(),
        };

        FileRange { range, file_id }
    }

    /// Returns the original file range that best describes the location of this macro call.
    ///
    /// Here we try to roughly match what rustc does to improve diagnostics: fn-like macros
    /// get the whole `ast::MacroCall`, attribute macros get the attribute's range, and derives
    /// get only the specific derive that is being referred to.
    pub fn original_call_range(self, db: &dyn ExpandDatabase) -> FileRange {
        let mut kind = self;
        let file_id = loop {
            match kind.file_id().repr() {
                HirFileIdRepr::MacroFile(file) => {
                    kind = db.lookup_intern_macro_call(file.macro_call_id).kind;
                }
                HirFileIdRepr::FileId(file_id) => break file_id,
            }
        };

        let range = match kind {
            MacroCallKind::FnLike { ast_id, .. } => ast_id.to_ptr(db).text_range(),
            MacroCallKind::Derive { ast_id, derive_attr_index, .. } => {
                // FIXME: should be the range of the macro name, not the whole derive
                // FIXME: handle `cfg_attr`
                collect_attrs(&ast_id.to_node(db))
                    .nth(derive_attr_index.ast_index())
                    .expect("missing derive")
                    .1
                    .expect_left("derive is a doc comment?")
                    .syntax()
                    .text_range()
            }
            // FIXME: handle `cfg_attr`
            MacroCallKind::Attr { ast_id, invoc_attr_index, .. } => {
                collect_attrs(&ast_id.to_node(db))
                    .nth(invoc_attr_index.ast_index())
                    .expect("missing attribute")
                    .1
                    .expect_left("attribute macro is a doc comment?")
                    .syntax()
                    .text_range()
            }
        };

        FileRange { range, file_id }
    }

    fn arg(&self, db: &dyn ExpandDatabase) -> InFile<Option<SyntaxNode>> {
        match self {
            MacroCallKind::FnLike { ast_id, .. } => {
                ast_id.to_in_file_node(db).map(|it| Some(it.token_tree()?.syntax().clone()))
            }
            MacroCallKind::Derive { ast_id, .. } => {
                ast_id.to_in_file_node(db).syntax().cloned().map(Some)
            }
            MacroCallKind::Attr { ast_id, .. } => {
                ast_id.to_in_file_node(db).syntax().cloned().map(Some)
            }
        }
    }
}

/// ExpansionInfo mainly describes how to map text range between src and expanded macro
// FIXME: can be expensive to create, we should check the use sites and maybe replace them with
// simpler function calls if the map is only used once
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpansionInfo {
    pub expanded: InMacroFile<SyntaxNode>,
    /// The argument TokenTree or item for attributes
    arg: InFile<Option<SyntaxNode>>,
    /// The `macro_rules!` or attribute input.
    attr_input_or_mac_def: Option<InFile<ast::TokenTree>>,

    macro_def: TokenExpander,
    macro_arg: Arc<tt::Subtree>,
    pub exp_map: Arc<ExpansionSpanMap>,
    arg_map: SpanMap,
}

impl ExpansionInfo {
    pub fn expanded(&self) -> InMacroFile<SyntaxNode> {
        self.expanded.clone()
    }

    pub fn call_node(&self) -> Option<InFile<SyntaxNode>> {
        Some(self.arg.with_value(self.arg.value.as_ref()?.parent()?))
    }

    /// Maps the passed in file range down into a macro expansion if it is the input to a macro call.
    pub fn map_range_down(
        &self,
        span: Span,
    ) -> Option<InMacroFile<impl Iterator<Item = SyntaxToken> + '_>> {
        let tokens = self
            .exp_map
            .ranges_with_span(span)
            .flat_map(move |range| self.expanded.value.covering_element(range).into_token());

        Some(InMacroFile::new(self.expanded.file_id, tokens))
    }

    /// Looks up the span at the given offset.
    pub fn span_for_offset(
        &self,
        db: &dyn ExpandDatabase,
        offset: TextSize,
    ) -> (FileRange, SyntaxContextId) {
        debug_assert!(self.expanded.value.text_range().contains(offset));
        let span = self.exp_map.span_at(offset);
        let anchor_offset = db
            .ast_id_map(span.anchor.file_id.into())
            .get_erased(span.anchor.ast_id)
            .text_range()
            .start();
        (FileRange { file_id: span.anchor.file_id, range: span.range + anchor_offset }, span.ctx)
    }

    /// Maps up the text range out of the expansion hierarchy back into the original file its from.
    pub fn map_node_range_up(
        &self,
        db: &dyn ExpandDatabase,
        range: TextRange,
    ) -> Option<(FileRange, SyntaxContextId)> {
        debug_assert!(self.expanded.value.text_range().contains_range(range));
        let mut spans = self.exp_map.spans_for_range(range);
        let Span { range, anchor, ctx } = spans.next()?;
        let mut start = range.start();
        let mut end = range.end();

        for span in spans {
            if span.anchor != anchor || span.ctx != ctx {
                return None;
            }
            start = start.min(span.range.start());
            end = end.max(span.range.end());
        }
        let anchor_offset =
            db.ast_id_map(anchor.file_id.into()).get_erased(anchor.ast_id).text_range().start();
        Some((
            FileRange {
                file_id: anchor.file_id,
                range: TextRange::new(start, end) + anchor_offset,
            },
            ctx,
        ))
    }

    /// Maps up the text range out of the expansion into is macro call.
    pub fn map_range_up_once(
        &self,
        db: &dyn ExpandDatabase,
        token: TextRange,
    ) -> InFile<smallvec::SmallVec<[TextRange; 1]>> {
        debug_assert!(self.expanded.value.text_range().contains_range(token));
        let span = self.exp_map.span_at(token.start());
        match &self.arg_map {
            SpanMap::RealSpanMap(_) => {
                let file_id = span.anchor.file_id.into();
                let anchor_offset =
                    db.ast_id_map(file_id).get_erased(span.anchor.ast_id).text_range().start();
                InFile { file_id, value: smallvec::smallvec![span.range + anchor_offset] }
            }
            SpanMap::ExpansionSpanMap(arg_map) => {
                let arg_range = self
                    .arg
                    .value
                    .as_ref()
                    .map_or_else(|| TextRange::empty(TextSize::from(0)), |it| it.text_range());
                InFile::new(
                    self.arg.file_id,
                    arg_map
                        .ranges_with_span(span)
                        .filter(|range| range.intersect(arg_range).is_some())
                        .collect(),
                )
            }
        }
    }

    pub fn new(db: &dyn ExpandDatabase, macro_file: MacroFileId) -> ExpansionInfo {
        let loc: MacroCallLoc = db.lookup_intern_macro_call(macro_file.macro_call_id);

        let arg_tt = loc.kind.arg(db);
        let arg_map = db.span_map(arg_tt.file_id);

        let macro_def = db.macro_expander(loc.def);
        let (parse, exp_map) = db.parse_macro_expansion(macro_file).value;
        let expanded = InMacroFile { file_id: macro_file, value: parse.syntax_node() };

        let (macro_arg, _) = db.macro_arg(macro_file.macro_call_id).value.unwrap_or_else(|| {
            (
                Arc::new(tt::Subtree {
                    delimiter: tt::Delimiter::invisible_spanned(loc.call_site),
                    token_trees: Vec::new(),
                }),
                SyntaxFixupUndoInfo::NONE,
            )
        });

        let def = loc.def.ast_id().left().and_then(|id| {
            let def_tt = match id.to_node(db) {
                ast::Macro::MacroRules(mac) => mac.token_tree()?,
                ast::Macro::MacroDef(_) if matches!(macro_def, TokenExpander::BuiltInAttr(_)) => {
                    return None
                }
                ast::Macro::MacroDef(mac) => mac.body()?,
            };
            Some(InFile::new(id.file_id, def_tt))
        });
        let attr_input_or_mac_def = def.or_else(|| match loc.kind {
            MacroCallKind::Attr { ast_id, invoc_attr_index, .. } => {
                // FIXME: handle `cfg_attr`
                let tt = collect_attrs(&ast_id.to_node(db))
                    .nth(invoc_attr_index.ast_index())
                    .and_then(|x| Either::left(x.1))?
                    .token_tree()?;
                Some(InFile::new(ast_id.file_id, tt))
            }
            _ => None,
        });

        ExpansionInfo {
            expanded,
            arg: arg_tt,
            attr_input_or_mac_def,
            macro_arg,
            macro_def,
            exp_map,
            arg_map,
        }
    }
}

/// In Rust, macros expand token trees to token trees. When we want to turn a
/// token tree into an AST node, we need to figure out what kind of AST node we
/// want: something like `foo` can be a type, an expression, or a pattern.
///
/// Naively, one would think that "what this expands to" is a property of a
/// particular macro: macro `m1` returns an item, while macro `m2` returns an
/// expression, etc. That's not the case -- macros are polymorphic in the
/// result, and can expand to any type of the AST node.
///
/// What defines the actual AST node is the syntactic context of the macro
/// invocation. As a contrived example, in `let T![*] = T![*];` the first `T`
/// expands to a pattern, while the second one expands to an expression.
///
/// `ExpandTo` captures this bit of information about a particular macro call
/// site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExpandTo {
    Statements,
    Items,
    Pattern,
    Type,
    Expr,
}

impl ExpandTo {
    pub fn from_call_site(call: &ast::MacroCall) -> ExpandTo {
        use syntax::SyntaxKind::*;

        let syn = call.syntax();

        let parent = match syn.parent() {
            Some(it) => it,
            None => return ExpandTo::Statements,
        };

        // FIXME: macros in statement position are treated as expression statements, they should
        // probably be their own statement kind. The *grand*parent indicates what's valid.
        if parent.kind() == MACRO_EXPR
            && parent
                .parent()
                .map_or(false, |p| matches!(p.kind(), EXPR_STMT | STMT_LIST | MACRO_STMTS))
        {
            return ExpandTo::Statements;
        }

        match parent.kind() {
            MACRO_ITEMS | SOURCE_FILE | ITEM_LIST => ExpandTo::Items,
            MACRO_STMTS | EXPR_STMT | STMT_LIST => ExpandTo::Statements,
            MACRO_PAT => ExpandTo::Pattern,
            MACRO_TYPE => ExpandTo::Type,

            ARG_LIST | ARRAY_EXPR | AWAIT_EXPR | BIN_EXPR | BREAK_EXPR | CALL_EXPR | CAST_EXPR
            | CLOSURE_EXPR | FIELD_EXPR | FOR_EXPR | IF_EXPR | INDEX_EXPR | LET_EXPR
            | MATCH_ARM | MATCH_EXPR | MATCH_GUARD | METHOD_CALL_EXPR | PAREN_EXPR | PATH_EXPR
            | PREFIX_EXPR | RANGE_EXPR | RECORD_EXPR_FIELD | REF_EXPR | RETURN_EXPR | TRY_EXPR
            | TUPLE_EXPR | WHILE_EXPR | MACRO_EXPR => ExpandTo::Expr,
            _ => {
                // Unknown , Just guess it is `Items`
                ExpandTo::Items
            }
        }
    }
}

intern::impl_internable!(ModPath, attrs::AttrInput);
