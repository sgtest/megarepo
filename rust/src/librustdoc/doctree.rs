//! This module is used to store stuff from Rust's AST in a more convenient
//! manner (and with prettier names) before cleaning.
crate use self::StructType::*;

use rustc_ast as ast;
use rustc_span::hygiene::MacroKind;
use rustc_span::{self, symbol::Ident, Span, Symbol};

use rustc_hir as hir;
use rustc_hir::def_id::CrateNum;
use rustc_hir::HirId;

crate struct Module<'hir> {
    crate name: Option<Symbol>,
    crate attrs: &'hir [ast::Attribute],
    crate where_outer: Span,
    crate where_inner: Span,
    crate extern_crates: Vec<ExternCrate<'hir>>,
    crate imports: Vec<Import<'hir>>,
    crate fns: Vec<Function<'hir>>,
    crate mods: Vec<Module<'hir>>,
    crate id: hir::HirId,
    // (item, renamed)
    crate items: Vec<(&'hir hir::Item<'hir>, Option<Ident>)>,
    crate traits: Vec<Trait<'hir>>,
    crate impls: Vec<Impl<'hir>>,
    crate foreigns: Vec<ForeignItem<'hir>>,
    crate macros: Vec<Macro>,
    crate proc_macros: Vec<ProcMacro>,
    crate is_crate: bool,
}

impl Module<'hir> {
    crate fn new(name: Option<Symbol>, attrs: &'hir [ast::Attribute]) -> Module<'hir> {
        Module {
            name,
            id: hir::CRATE_HIR_ID,
            where_outer: rustc_span::DUMMY_SP,
            where_inner: rustc_span::DUMMY_SP,
            attrs,
            extern_crates: Vec::new(),
            imports: Vec::new(),
            fns: Vec::new(),
            mods: Vec::new(),
            items: Vec::new(),
            traits: Vec::new(),
            impls: Vec::new(),
            foreigns: Vec::new(),
            macros: Vec::new(),
            proc_macros: Vec::new(),
            is_crate: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
crate enum StructType {
    /// A braced struct
    Plain,
    /// A tuple struct
    Tuple,
    /// A unit struct
    Unit,
}

crate struct Variant<'hir> {
    crate name: Symbol,
    crate id: hir::HirId,
    crate def: &'hir hir::VariantData<'hir>,
}

crate struct Function<'hir> {
    crate decl: &'hir hir::FnDecl<'hir>,
    crate id: hir::HirId,
    crate name: Symbol,
    crate header: hir::FnHeader,
    crate generics: &'hir hir::Generics<'hir>,
    crate body: hir::BodyId,
}

crate struct Trait<'hir> {
    crate is_auto: hir::IsAuto,
    crate unsafety: hir::Unsafety,
    crate name: Symbol,
    crate items: Vec<&'hir hir::TraitItem<'hir>>,
    crate generics: &'hir hir::Generics<'hir>,
    crate bounds: &'hir [hir::GenericBound<'hir>],
    crate attrs: &'hir [ast::Attribute],
    crate id: hir::HirId,
}

#[derive(Debug)]
crate struct Impl<'hir> {
    crate unsafety: hir::Unsafety,
    crate polarity: hir::ImplPolarity,
    crate defaultness: hir::Defaultness,
    crate constness: hir::Constness,
    crate generics: &'hir hir::Generics<'hir>,
    crate trait_: &'hir Option<hir::TraitRef<'hir>>,
    crate for_: &'hir hir::Ty<'hir>,
    crate items: Vec<&'hir hir::ImplItem<'hir>>,
    crate attrs: &'hir [ast::Attribute],
    crate span: Span,
    crate vis: &'hir hir::Visibility<'hir>,
    crate id: hir::HirId,
}

crate struct ForeignItem<'hir> {
    crate id: hir::HirId,
    crate name: Symbol,
    crate kind: &'hir hir::ForeignItemKind<'hir>,
}

// For Macro we store the DefId instead of the NodeId, since we also create
// these imported macro_rules (which only have a DUMMY_NODE_ID).
crate struct Macro {
    crate name: Symbol,
    crate def_id: hir::def_id::DefId,
    crate matchers: Vec<Span>,
    crate imported_from: Option<Symbol>,
}

crate struct ExternCrate<'hir> {
    crate name: Symbol,
    crate hir_id: HirId,
    crate cnum: CrateNum,
    crate path: Option<String>,
    crate vis: &'hir hir::Visibility<'hir>,
    crate attrs: &'hir [ast::Attribute],
    crate span: Span,
}

#[derive(Debug)]
crate struct Import<'hir> {
    crate name: Symbol,
    crate id: hir::HirId,
    crate vis: &'hir hir::Visibility<'hir>,
    crate attrs: &'hir [ast::Attribute],
    crate path: &'hir hir::Path<'hir>,
    crate glob: bool,
    crate span: Span,
}

crate struct ProcMacro {
    crate name: Symbol,
    crate id: hir::HirId,
    crate kind: MacroKind,
    crate helpers: Vec<Symbol>,
}

crate fn struct_type_from_def(vdata: &hir::VariantData<'_>) -> StructType {
    match *vdata {
        hir::VariantData::Struct(..) => Plain,
        hir::VariantData::Tuple(..) => Tuple,
        hir::VariantData::Unit(..) => Unit,
    }
}
