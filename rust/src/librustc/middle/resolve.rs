// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(non_camel_case_types)]

pub use self::PrivateDep::*;
pub use self::ImportUse::*;
pub use self::TraitItemKind::*;
pub use self::LastPrivate::*;
use self::PatternBindingMode::*;
use self::Namespace::*;
use self::NamespaceError::*;
use self::NamespaceResult::*;
use self::NameDefinition::*;
use self::ImportDirectiveSubclass::*;
use self::ReducedGraphParent::*;
use self::ResolveResult::*;
use self::FallbackSuggestion::*;
use self::TypeParameters::*;
use self::RibKind::*;
use self::MethodSort::*;
use self::UseLexicalScopeFlag::*;
use self::ModulePrefixResult::*;
use self::NameSearchType::*;
use self::BareIdentifierPatternResolution::*;
use self::DuplicateCheckingMode::*;
use self::ParentLink::*;
use self::ModuleKind::*;
use self::TraitReferenceType::*;
use self::FallbackChecks::*;

use session::Session;
use lint;
use metadata::csearch;
use metadata::decoder::{DefLike, DlDef, DlField, DlImpl};
use middle::def::*;
use middle::lang_items::LanguageItems;
use middle::pat_util::pat_bindings;
use middle::subst::{ParamSpace, FnSpace, TypeSpace};
use middle::ty::{ExplicitSelfCategory, StaticExplicitSelfCategory};
use middle::ty::{CaptureModeMap, Freevar, FreevarMap};
use util::nodemap::{NodeMap, NodeSet, DefIdSet, FnvHashMap};

use syntax::ast::{Arm, BindByRef, BindByValue, BindingMode, Block, Crate, CrateNum};
use syntax::ast::{DeclItem, DefId, Expr, ExprAgain, ExprBreak, ExprField};
use syntax::ast::{ExprClosure, ExprForLoop, ExprLoop, ExprWhile, ExprMethodCall};
use syntax::ast::{ExprPath, ExprProc, ExprStruct, FnDecl};
use syntax::ast::{ForeignItem, ForeignItemFn, ForeignItemStatic, Generics};
use syntax::ast::{Ident, ImplItem, Item, ItemEnum, ItemFn, ItemForeignMod};
use syntax::ast::{ItemImpl, ItemMac, ItemMod, ItemStatic, ItemStruct};
use syntax::ast::{ItemTrait, ItemTy, LOCAL_CRATE, Local, ItemConst};
use syntax::ast::{MethodImplItem, Mod, Name, NamedField, NodeId};
use syntax::ast::{Pat, PatEnum, PatIdent, PatLit};
use syntax::ast::{PatRange, PatStruct, Path, PathListIdent, PathListMod};
use syntax::ast::{PolyTraitRef, PrimTy, Public, SelfExplicit, SelfStatic};
use syntax::ast::{RegionTyParamBound, StmtDecl, StructField};
use syntax::ast::{StructVariantKind, TraitRef, TraitTyParamBound};
use syntax::ast::{TupleVariantKind, Ty, TyBool, TyChar, TyClosure, TyF32};
use syntax::ast::{TyF64, TyFloat, TyI, TyI8, TyI16, TyI32, TyI64, TyInt, TyObjectSum};
use syntax::ast::{TyParam, TyParamBound, TyPath, TyPtr, TyPolyTraitRef, TyProc, TyQPath};
use syntax::ast::{TyRptr, TyStr, TyU, TyU8, TyU16, TyU32, TyU64, TyUint};
use syntax::ast::{TypeImplItem, UnnamedField};
use syntax::ast::{Variant, ViewItem, ViewItemExternCrate};
use syntax::ast::{ViewItemUse, ViewPathGlob, ViewPathList, ViewPathSimple};
use syntax::ast::{Visibility};
use syntax::ast;
use syntax::ast_util::{mod, PostExpansionMethod, local_def, walk_pat};
use syntax::attr::AttrMetaMethods;
use syntax::ext::mtwt;
use syntax::parse::token::{mod, special_names, special_idents};
use syntax::codemap::{Span, DUMMY_SP, Pos};
use syntax::owned_slice::OwnedSlice;
use syntax::visit::{mod, Visitor};

use std::collections::{HashMap, HashSet};
use std::collections::hash_map::{Occupied, Vacant};
use std::cell::{Cell, RefCell};
use std::mem::replace;
use std::rc::{Rc, Weak};
use std::uint;

// Definition mapping
pub type DefMap = RefCell<NodeMap<Def>>;

struct binding_info {
    span: Span,
    binding_mode: BindingMode,
}

impl Copy for binding_info {}

// Map from the name in a pattern to its binding mode.
type BindingMap = HashMap<Name,binding_info>;

// Trait method resolution
pub type TraitMap = NodeMap<Vec<DefId> >;

// This is the replacement export map. It maps a module to all of the exports
// within.
pub type ExportMap2 = NodeMap<Vec<Export2>>;

pub struct Export2 {
    pub name: String,        // The name of the target.
    pub def_id: DefId,     // The definition of the target.
}

// This set contains all exported definitions from external crates. The set does
// not contain any entries from local crates.
pub type ExternalExports = DefIdSet;

// FIXME: dox
pub type LastPrivateMap = NodeMap<LastPrivate>;

#[deriving(Show)]
pub enum LastPrivate {
    LastMod(PrivateDep),
    // `use` directives (imports) can refer to two separate definitions in the
    // type and value namespaces. We record here the last private node for each
    // and whether the import is in fact used for each.
    // If the Option<PrivateDep> fields are None, it means there is no definition
    // in that namespace.
    LastImport{value_priv: Option<PrivateDep>,
               value_used: ImportUse,
               type_priv: Option<PrivateDep>,
               type_used: ImportUse},
}

impl Copy for LastPrivate {}

#[deriving(Show)]
pub enum PrivateDep {
    AllPublic,
    DependsOn(DefId),
}

impl Copy for PrivateDep {}

// How an import is used.
#[deriving(PartialEq, Show)]
pub enum ImportUse {
    Unused,       // The import is not used.
    Used,         // The import is used.
}

impl Copy for ImportUse {}

impl LastPrivate {
    fn or(self, other: LastPrivate) -> LastPrivate {
        match (self, other) {
            (me, LastMod(AllPublic)) => me,
            (_, other) => other,
        }
    }
}

#[deriving(PartialEq)]
enum PatternBindingMode {
    RefutableMode,
    LocalIrrefutableMode,
    ArgumentIrrefutableMode,
}

impl Copy for PatternBindingMode {}

#[deriving(PartialEq, Eq, Hash, Show)]
enum Namespace {
    TypeNS,
    ValueNS
}

impl Copy for Namespace {}

#[deriving(PartialEq)]
enum NamespaceError {
    NoError,
    ModuleError,
    TypeError,
    ValueError
}

impl Copy for NamespaceError {}

/// A NamespaceResult represents the result of resolving an import in
/// a particular namespace. The result is either definitely-resolved,
/// definitely- unresolved, or unknown.
#[deriving(Clone)]
enum NamespaceResult {
    /// Means that resolve hasn't gathered enough information yet to determine
    /// whether the name is bound in this namespace. (That is, it hasn't
    /// resolved all `use` directives yet.)
    UnknownResult,
    /// Means that resolve has determined that the name is definitely
    /// not bound in the namespace.
    UnboundResult,
    /// Means that resolve has determined that the name is bound in the Module
    /// argument, and specified by the NameBindings argument.
    BoundResult(Rc<Module>, Rc<NameBindings>)
}

impl NamespaceResult {
    fn is_unknown(&self) -> bool {
        match *self {
            UnknownResult => true,
            _ => false
        }
    }
    fn is_unbound(&self) -> bool {
        match *self {
            UnboundResult => true,
            _ => false
        }
    }
}

enum NameDefinition {
    NoNameDefinition,           //< The name was unbound.
    ChildNameDefinition(Def, LastPrivate), //< The name identifies an immediate child.
    ImportNameDefinition(Def, LastPrivate) //< The name identifies an import.
}

impl<'a, 'v> Visitor<'v> for Resolver<'a> {
    fn visit_item(&mut self, item: &Item) {
        self.resolve_item(item);
    }
    fn visit_arm(&mut self, arm: &Arm) {
        self.resolve_arm(arm);
    }
    fn visit_block(&mut self, block: &Block) {
        self.resolve_block(block);
    }
    fn visit_expr(&mut self, expr: &Expr) {
        self.resolve_expr(expr);
    }
    fn visit_local(&mut self, local: &Local) {
        self.resolve_local(local);
    }
    fn visit_ty(&mut self, ty: &Ty) {
        self.resolve_type(ty);
    }
}

/// Contains data for specific types of import directives.
enum ImportDirectiveSubclass {
    SingleImport(Name /* target */, Name /* source */),
    GlobImport
}

impl Copy for ImportDirectiveSubclass {}

/// The context that we thread through while building the reduced graph.
#[deriving(Clone)]
enum ReducedGraphParent {
    ModuleReducedGraphParent(Rc<Module>)
}

impl ReducedGraphParent {
    fn module(&self) -> Rc<Module> {
        match *self {
            ModuleReducedGraphParent(ref m) => {
                m.clone()
            }
        }
    }
}

type ErrorMessage = Option<(Span, String)>;

enum ResolveResult<T> {
    Failed(ErrorMessage),   // Failed to resolve the name, optional helpful error message.
    Indeterminate,          // Couldn't determine due to unresolved globs.
    Success(T)              // Successfully resolved the import.
}

impl<T> ResolveResult<T> {
    fn indeterminate(&self) -> bool {
        match *self { Indeterminate => true, _ => false }
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

enum TypeParameters<'a> {
    NoTypeParameters,
    HasTypeParameters(
        // Type parameters.
        &'a Generics,

        // Identifies the things that these parameters
        // were declared on (type, fn, etc)
        ParamSpace,

        // ID of the enclosing item.
        NodeId,

        // The kind of the rib used for type parameters.
        RibKind)
}

impl<'a> Copy for TypeParameters<'a> {}

// The rib kind controls the translation of local
// definitions (`DefLocal`) to upvars (`DefUpvar`).

enum RibKind {
    // No translation needs to be applied.
    NormalRibKind,

    // We passed through a closure scope at the given node ID.
    // Translate upvars as appropriate.
    ClosureRibKind(NodeId /* func id */, NodeId /* body id if proc or unboxed */),

    // We passed through an impl or trait and are now in one of its
    // methods. Allow references to ty params that impl or trait
    // binds. Disallow any other upvars (including other ty params that are
    // upvars).
              // parent;   method itself
    MethodRibKind(NodeId, MethodSort),

    // We passed through an item scope. Disallow upvars.
    ItemRibKind,

    // We're in a constant item. Can't refer to dynamic stuff.
    ConstantItemRibKind
}

impl Copy for RibKind {}

// Methods can be required or provided. RequiredMethod methods only occur in traits.
enum MethodSort {
    RequiredMethod,
    ProvidedMethod(NodeId)
}

impl Copy for MethodSort {}

enum UseLexicalScopeFlag {
    DontUseLexicalScope,
    UseLexicalScope
}

impl Copy for UseLexicalScopeFlag {}

enum ModulePrefixResult {
    NoPrefixFound,
    PrefixFound(Rc<Module>, uint)
}

#[deriving(Clone, Eq, PartialEq)]
pub enum TraitItemKind {
    NonstaticMethodTraitItemKind,
    StaticMethodTraitItemKind,
    TypeTraitItemKind,
}

impl Copy for TraitItemKind {}

impl TraitItemKind {
    pub fn from_explicit_self_category(explicit_self_category:
                                       ExplicitSelfCategory)
                                       -> TraitItemKind {
        if explicit_self_category == StaticExplicitSelfCategory {
            StaticMethodTraitItemKind
        } else {
            NonstaticMethodTraitItemKind
        }
    }
}

#[deriving(PartialEq)]
enum NameSearchType {
    /// We're doing a name search in order to resolve a `use` directive.
    ImportSearch,

    /// We're doing a name search in order to resolve a path type, a path
    /// expression, or a path pattern.
    PathSearch,
}

impl Copy for NameSearchType {}

enum BareIdentifierPatternResolution {
    FoundStructOrEnumVariant(Def, LastPrivate),
    FoundConst(Def, LastPrivate),
    BareIdentifierPatternUnresolved
}

impl Copy for BareIdentifierPatternResolution {}

// Specifies how duplicates should be handled when adding a child item if
// another item exists with the same name in some namespace.
#[deriving(PartialEq)]
enum DuplicateCheckingMode {
    ForbidDuplicateModules,
    ForbidDuplicateTypesAndModules,
    ForbidDuplicateValues,
    ForbidDuplicateTypesAndValues,
    OverwriteDuplicates
}

impl Copy for DuplicateCheckingMode {}

/// One local scope.
struct Rib {
    bindings: HashMap<Name, DefLike>,
    kind: RibKind,
}

impl Rib {
    fn new(kind: RibKind) -> Rib {
        Rib {
            bindings: HashMap::new(),
            kind: kind
        }
    }
}

/// One import directive.
struct ImportDirective {
    module_path: Vec<Name>,
    subclass: ImportDirectiveSubclass,
    span: Span,
    id: NodeId,
    is_public: bool, // see note in ImportResolution about how to use this
    shadowable: bool,
}

impl ImportDirective {
    fn new(module_path: Vec<Name> ,
           subclass: ImportDirectiveSubclass,
           span: Span,
           id: NodeId,
           is_public: bool,
           shadowable: bool)
           -> ImportDirective {
        ImportDirective {
            module_path: module_path,
            subclass: subclass,
            span: span,
            id: id,
            is_public: is_public,
            shadowable: shadowable,
        }
    }
}

/// The item that an import resolves to.
#[deriving(Clone)]
struct Target {
    target_module: Rc<Module>,
    bindings: Rc<NameBindings>,
    shadowable: bool,
}

impl Target {
    fn new(target_module: Rc<Module>,
           bindings: Rc<NameBindings>,
           shadowable: bool)
           -> Target {
        Target {
            target_module: target_module,
            bindings: bindings,
            shadowable: shadowable,
        }
    }
}

/// An ImportResolution represents a particular `use` directive.
struct ImportResolution {
    /// Whether this resolution came from a `use` or a `pub use`. Note that this
    /// should *not* be used whenever resolution is being performed, this is
    /// only looked at for glob imports statements currently. Privacy testing
    /// occurs during a later phase of compilation.
    is_public: bool,

    // The number of outstanding references to this name. When this reaches
    // zero, outside modules can count on the targets being correct. Before
    // then, all bets are off; future imports could override this name.
    outstanding_references: uint,

    /// The value that this `use` directive names, if there is one.
    value_target: Option<Target>,
    /// The source node of the `use` directive leading to the value target
    /// being non-none
    value_id: NodeId,

    /// The type that this `use` directive names, if there is one.
    type_target: Option<Target>,
    /// The source node of the `use` directive leading to the type target
    /// being non-none
    type_id: NodeId,
}

impl ImportResolution {
    fn new(id: NodeId, is_public: bool) -> ImportResolution {
        ImportResolution {
            type_id: id,
            value_id: id,
            outstanding_references: 0,
            value_target: None,
            type_target: None,
            is_public: is_public,
        }
    }

    fn target_for_namespace(&self, namespace: Namespace)
                                -> Option<Target> {
        match namespace {
            TypeNS  => self.type_target.clone(),
            ValueNS => self.value_target.clone(),
        }
    }

    fn id(&self, namespace: Namespace) -> NodeId {
        match namespace {
            TypeNS  => self.type_id,
            ValueNS => self.value_id,
        }
    }
}

/// The link from a module up to its nearest parent node.
#[deriving(Clone)]
enum ParentLink {
    NoParentLink,
    ModuleParentLink(Weak<Module>, Name),
    BlockParentLink(Weak<Module>, NodeId)
}

/// The type of module this is.
#[deriving(PartialEq)]
enum ModuleKind {
    NormalModuleKind,
    TraitModuleKind,
    ImplModuleKind,
    EnumModuleKind,
    AnonymousModuleKind,
}

impl Copy for ModuleKind {}

/// One node in the tree of modules.
struct Module {
    parent_link: ParentLink,
    def_id: Cell<Option<DefId>>,
    kind: Cell<ModuleKind>,
    is_public: bool,

    children: RefCell<HashMap<Name, Rc<NameBindings>>>,
    imports: RefCell<Vec<ImportDirective>>,

    // The external module children of this node that were declared with
    // `extern crate`.
    external_module_children: RefCell<HashMap<Name, Rc<Module>>>,

    // The anonymous children of this node. Anonymous children are pseudo-
    // modules that are implicitly created around items contained within
    // blocks.
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
    anonymous_children: RefCell<NodeMap<Rc<Module>>>,

    // The status of resolving each import in this module.
    import_resolutions: RefCell<HashMap<Name, ImportResolution>>,

    // The number of unresolved globs that this module exports.
    glob_count: Cell<uint>,

    // The index of the import we're resolving.
    resolved_import_count: Cell<uint>,

    // Whether this module is populated. If not populated, any attempt to
    // access the children must be preceded with a
    // `populate_module_if_necessary` call.
    populated: Cell<bool>,
}

impl Module {
    fn new(parent_link: ParentLink,
           def_id: Option<DefId>,
           kind: ModuleKind,
           external: bool,
           is_public: bool)
           -> Module {
        Module {
            parent_link: parent_link,
            def_id: Cell::new(def_id),
            kind: Cell::new(kind),
            is_public: is_public,
            children: RefCell::new(HashMap::new()),
            imports: RefCell::new(Vec::new()),
            external_module_children: RefCell::new(HashMap::new()),
            anonymous_children: RefCell::new(NodeMap::new()),
            import_resolutions: RefCell::new(HashMap::new()),
            glob_count: Cell::new(0),
            resolved_import_count: Cell::new(0),
            populated: Cell::new(!external),
        }
    }

    fn all_imports_resolved(&self) -> bool {
        self.imports.borrow().len() == self.resolved_import_count.get()
    }
}

bitflags! {
    #[deriving(Show)]
    flags DefModifiers: u8 {
        const PUBLIC            = 0b0000_0001,
        const IMPORTABLE        = 0b0000_0010,
    }
}

impl Copy for DefModifiers {}

// Records a possibly-private type definition.
#[deriving(Clone)]
struct TypeNsDef {
    modifiers: DefModifiers, // see note in ImportResolution about how to use this
    module_def: Option<Rc<Module>>,
    type_def: Option<Def>,
    type_span: Option<Span>
}

// Records a possibly-private value definition.
#[deriving(Clone, Show)]
struct ValueNsDef {
    modifiers: DefModifiers, // see note in ImportResolution about how to use this
    def: Def,
    value_span: Option<Span>,
}

impl Copy for ValueNsDef {}

// Records the definitions (at most one for each namespace) that a name is
// bound to.
struct NameBindings {
    type_def: RefCell<Option<TypeNsDef>>,   //< Meaning in type namespace.
    value_def: RefCell<Option<ValueNsDef>>, //< Meaning in value namespace.
}

/// Ways in which a trait can be referenced
enum TraitReferenceType {
    TraitImplementation,             // impl SomeTrait for T { ... }
    TraitDerivation,                 // trait T : SomeTrait { ... }
    TraitBoundingTypeParameter,      // fn f<T:SomeTrait>() { ... }
    TraitObject,                     // Box<for<'a> SomeTrait>
    TraitQPath,                      // <T as SomeTrait>::
}

impl Copy for TraitReferenceType {}

impl NameBindings {
    fn new() -> NameBindings {
        NameBindings {
            type_def: RefCell::new(None),
            value_def: RefCell::new(None),
        }
    }

    /// Creates a new module in this set of name bindings.
    fn define_module(&self,
                     parent_link: ParentLink,
                     def_id: Option<DefId>,
                     kind: ModuleKind,
                     external: bool,
                     is_public: bool,
                     sp: Span) {
        // Merges the module with the existing type def or creates a new one.
        let modifiers = if is_public { PUBLIC } else { DefModifiers::empty() } | IMPORTABLE;
        let module_ = Rc::new(Module::new(parent_link,
                                          def_id,
                                          kind,
                                          external,
                                          is_public));
        let type_def = self.type_def.borrow().clone();
        match type_def {
            None => {
                *self.type_def.borrow_mut() = Some(TypeNsDef {
                    modifiers: modifiers,
                    module_def: Some(module_),
                    type_def: None,
                    type_span: Some(sp)
                });
            }
            Some(type_def) => {
                *self.type_def.borrow_mut() = Some(TypeNsDef {
                    modifiers: modifiers,
                    module_def: Some(module_),
                    type_span: Some(sp),
                    type_def: type_def.type_def
                });
            }
        }
    }

    /// Sets the kind of the module, creating a new one if necessary.
    fn set_module_kind(&self,
                       parent_link: ParentLink,
                       def_id: Option<DefId>,
                       kind: ModuleKind,
                       external: bool,
                       is_public: bool,
                       _sp: Span) {
        let modifiers = if is_public { PUBLIC } else { DefModifiers::empty() } | IMPORTABLE;
        let type_def = self.type_def.borrow().clone();
        match type_def {
            None => {
                let module = Module::new(parent_link, def_id, kind,
                                         external, is_public);
                *self.type_def.borrow_mut() = Some(TypeNsDef {
                    modifiers: modifiers,
                    module_def: Some(Rc::new(module)),
                    type_def: None,
                    type_span: None,
                });
            }
            Some(type_def) => {
                match type_def.module_def {
                    None => {
                        let module = Module::new(parent_link,
                                                 def_id,
                                                 kind,
                                                 external,
                                                 is_public);
                        *self.type_def.borrow_mut() = Some(TypeNsDef {
                            modifiers: modifiers,
                            module_def: Some(Rc::new(module)),
                            type_def: type_def.type_def,
                            type_span: None,
                        });
                    }
                    Some(module_def) => module_def.kind.set(kind),
                }
            }
        }
    }

    /// Records a type definition.
    fn define_type(&self, def: Def, sp: Span, modifiers: DefModifiers) {
        debug!("defining type for def {} with modifiers {}", def, modifiers);
        // Merges the type with the existing type def or creates a new one.
        let type_def = self.type_def.borrow().clone();
        match type_def {
            None => {
                *self.type_def.borrow_mut() = Some(TypeNsDef {
                    module_def: None,
                    type_def: Some(def),
                    type_span: Some(sp),
                    modifiers: modifiers,
                });
            }
            Some(type_def) => {
                *self.type_def.borrow_mut() = Some(TypeNsDef {
                    type_def: Some(def),
                    type_span: Some(sp),
                    module_def: type_def.module_def,
                    modifiers: modifiers,
                });
            }
        }
    }

    /// Records a value definition.
    fn define_value(&self, def: Def, sp: Span, modifiers: DefModifiers) {
        debug!("defining value for def {} with modifiers {}", def, modifiers);
        *self.value_def.borrow_mut() = Some(ValueNsDef {
            def: def,
            value_span: Some(sp),
            modifiers: modifiers,
        });
    }

    /// Returns the module node if applicable.
    fn get_module_if_available(&self) -> Option<Rc<Module>> {
        match *self.type_def.borrow() {
            Some(ref type_def) => type_def.module_def.clone(),
            None => None
        }
    }

    /// Returns the module node. Panics if this node does not have a module
    /// definition.
    fn get_module(&self) -> Rc<Module> {
        match self.get_module_if_available() {
            None => {
                panic!("get_module called on a node with no module \
                       definition!")
            }
            Some(module_def) => module_def
        }
    }

    fn defined_in_namespace(&self, namespace: Namespace) -> bool {
        match namespace {
            TypeNS   => return self.type_def.borrow().is_some(),
            ValueNS  => return self.value_def.borrow().is_some()
        }
    }

    fn defined_in_public_namespace(&self, namespace: Namespace) -> bool {
        self.defined_in_namespace_with(namespace, PUBLIC)
    }

    fn defined_in_namespace_with(&self, namespace: Namespace, modifiers: DefModifiers) -> bool {
        match namespace {
            TypeNS => match *self.type_def.borrow() {
                Some(ref def) => def.modifiers.contains(modifiers), None => false
            },
            ValueNS => match *self.value_def.borrow() {
                Some(ref def) => def.modifiers.contains(modifiers), None => false
            }
        }
    }

    fn def_for_namespace(&self, namespace: Namespace) -> Option<Def> {
        match namespace {
            TypeNS => {
                match *self.type_def.borrow() {
                    None => None,
                    Some(ref type_def) => {
                        match type_def.type_def {
                            Some(type_def) => Some(type_def),
                            None => {
                                match type_def.module_def {
                                    Some(ref module) => {
                                        match module.def_id.get() {
                                            Some(did) => Some(DefMod(did)),
                                            None => None,
                                        }
                                    }
                                    None => None,
                                }
                            }
                        }
                    }
                }
            }
            ValueNS => {
                match *self.value_def.borrow() {
                    None => None,
                    Some(value_def) => Some(value_def.def)
                }
            }
        }
    }

    fn span_for_namespace(&self, namespace: Namespace) -> Option<Span> {
        if self.defined_in_namespace(namespace) {
            match namespace {
                TypeNS  => {
                    match *self.type_def.borrow() {
                        None => None,
                        Some(ref type_def) => type_def.type_span
                    }
                }
                ValueNS => {
                    match *self.value_def.borrow() {
                        None => None,
                        Some(ref value_def) => value_def.value_span
                    }
                }
            }
        } else {
            None
        }
    }
}

/// Interns the names of the primitive types.
struct PrimitiveTypeTable {
    primitive_types: HashMap<Name, PrimTy>,
}

impl PrimitiveTypeTable {
    fn new() -> PrimitiveTypeTable {
        let mut table = PrimitiveTypeTable {
            primitive_types: HashMap::new()
        };

        table.intern("bool",    TyBool);
        table.intern("char",    TyChar);
        table.intern("f32",     TyFloat(TyF32));
        table.intern("f64",     TyFloat(TyF64));
        table.intern("int",     TyInt(TyI));
        table.intern("i8",      TyInt(TyI8));
        table.intern("i16",     TyInt(TyI16));
        table.intern("i32",     TyInt(TyI32));
        table.intern("i64",     TyInt(TyI64));
        table.intern("str",     TyStr);
        table.intern("uint",    TyUint(TyU));
        table.intern("u8",      TyUint(TyU8));
        table.intern("u16",     TyUint(TyU16));
        table.intern("u32",     TyUint(TyU32));
        table.intern("u64",     TyUint(TyU64));

        table
    }

    fn intern(&mut self, string: &str, primitive_type: PrimTy) {
        self.primitive_types.insert(token::intern(string), primitive_type);
    }
}


fn namespace_error_to_string(ns: NamespaceError) -> &'static str {
    match ns {
        NoError                 => "",
        ModuleError | TypeError => "type or module",
        ValueError              => "value",
    }
}

/// The main resolver class.
struct Resolver<'a> {
    session: &'a Session,

    graph_root: NameBindings,

    trait_item_map: FnvHashMap<(Name, DefId), TraitItemKind>,

    structs: FnvHashMap<DefId, Vec<Name>>,

    // The number of imports that are currently unresolved.
    unresolved_imports: uint,

    // The module that represents the current item scope.
    current_module: Rc<Module>,

    // The current set of local scopes, for values.
    // FIXME #4948: Reuse ribs to avoid allocation.
    value_ribs: Vec<Rib>,

    // The current set of local scopes, for types.
    type_ribs: Vec<Rib>,

    // The current set of local scopes, for labels.
    label_ribs: Vec<Rib>,

    // The trait that the current context can refer to.
    current_trait_ref: Option<(DefId, TraitRef)>,

    // The current self type if inside an impl (used for better errors).
    current_self_type: Option<Ty>,

    // The ident for the keyword "self".
    self_name: Name,
    // The ident for the non-keyword "Self".
    type_self_name: Name,

    // The idents for the primitive types.
    primitive_type_table: PrimitiveTypeTable,

    def_map: DefMap,
    freevars: RefCell<FreevarMap>,
    freevars_seen: RefCell<NodeMap<NodeSet>>,
    capture_mode_map: CaptureModeMap,
    export_map2: ExportMap2,
    trait_map: TraitMap,
    external_exports: ExternalExports,
    last_private: LastPrivateMap,

    // Whether or not to print error messages. Can be set to true
    // when getting additional info for error message suggestions,
    // so as to avoid printing duplicate errors
    emit_errors: bool,

    used_imports: HashSet<(NodeId, Namespace)>,
    used_crates: HashSet<CrateNum>,
}

struct BuildReducedGraphVisitor<'a, 'b:'a> {
    resolver: &'a mut Resolver<'b>,
    parent: ReducedGraphParent
}

impl<'a, 'b, 'v> Visitor<'v> for BuildReducedGraphVisitor<'a, 'b> {

    fn visit_item(&mut self, item: &Item) {
        let p = self.resolver.build_reduced_graph_for_item(item, self.parent.clone());
        let old_parent = replace(&mut self.parent, p);
        visit::walk_item(self, item);
        self.parent = old_parent;
    }

    fn visit_foreign_item(&mut self, foreign_item: &ForeignItem) {
        let parent = self.parent.clone();
        self.resolver.build_reduced_graph_for_foreign_item(foreign_item,
                                                           parent.clone(),
                                                           |r| {
            let mut v = BuildReducedGraphVisitor {
                resolver: r,
                parent: parent.clone()
            };
            visit::walk_foreign_item(&mut v, foreign_item);
        })
    }

    fn visit_view_item(&mut self, view_item: &ViewItem) {
        self.resolver.build_reduced_graph_for_view_item(view_item, self.parent.clone());
    }

    fn visit_block(&mut self, block: &Block) {
        let np = self.resolver.build_reduced_graph_for_block(block, self.parent.clone());
        let old_parent = replace(&mut self.parent, np);
        visit::walk_block(self, block);
        self.parent = old_parent;
    }

}

struct UnusedImportCheckVisitor<'a, 'b:'a> {
    resolver: &'a mut Resolver<'b>
}

impl<'a, 'b, 'v> Visitor<'v> for UnusedImportCheckVisitor<'a, 'b> {
    fn visit_view_item(&mut self, vi: &ViewItem) {
        self.resolver.check_for_item_unused_imports(vi);
        visit::walk_view_item(self, vi);
    }
}

#[deriving(PartialEq)]
enum FallbackChecks {
    Everything,
    OnlyTraitAndStatics
}


impl<'a> Resolver<'a> {
    fn new(session: &'a Session, crate_span: Span) -> Resolver<'a> {
        let graph_root = NameBindings::new();

        graph_root.define_module(NoParentLink,
                                 Some(DefId { krate: 0, node: 0 }),
                                 NormalModuleKind,
                                 false,
                                 true,
                                 crate_span);

        let current_module = graph_root.get_module();

        Resolver {
            session: session,

            // The outermost module has def ID 0; this is not reflected in the
            // AST.

            graph_root: graph_root,

            trait_item_map: FnvHashMap::new(),
            structs: FnvHashMap::new(),

            unresolved_imports: 0,

            current_module: current_module,
            value_ribs: Vec::new(),
            type_ribs: Vec::new(),
            label_ribs: Vec::new(),

            current_trait_ref: None,
            current_self_type: None,

            self_name: special_names::self_,
            type_self_name: special_names::type_self,

            primitive_type_table: PrimitiveTypeTable::new(),

            def_map: RefCell::new(NodeMap::new()),
            freevars: RefCell::new(NodeMap::new()),
            freevars_seen: RefCell::new(NodeMap::new()),
            capture_mode_map: NodeMap::new(),
            export_map2: NodeMap::new(),
            trait_map: NodeMap::new(),
            used_imports: HashSet::new(),
            used_crates: HashSet::new(),
            external_exports: DefIdSet::new(),
            last_private: NodeMap::new(),

            emit_errors: true,
        }
    }
    /// The main name resolution procedure.
    fn resolve(&mut self, krate: &ast::Crate) {
        self.build_reduced_graph(krate);
        self.session.abort_if_errors();

        self.resolve_imports();
        self.session.abort_if_errors();

        self.record_exports();
        self.session.abort_if_errors();

        self.resolve_crate(krate);
        self.session.abort_if_errors();

        self.check_for_unused_imports(krate);
    }

    //
    // Reduced graph building
    //
    // Here we build the "reduced graph": the graph of the module tree without
    // any imports resolved.
    //

    /// Constructs the reduced graph for the entire crate.
    fn build_reduced_graph(&mut self, krate: &ast::Crate) {
        let parent = ModuleReducedGraphParent(self.graph_root.get_module());
        let mut visitor = BuildReducedGraphVisitor {
            resolver: self,
            parent: parent
        };
        visit::walk_crate(&mut visitor, krate);
    }

    /// Adds a new child item to the module definition of the parent node and
    /// returns its corresponding name bindings as well as the current parent.
    /// Or, if we're inside a block, creates (or reuses) an anonymous module
    /// corresponding to the innermost block ID and returns the name bindings
    /// as well as the newly-created parent.
    ///
    /// # Panics
    ///
    /// Panics if this node does not have a module definition and we are not inside
    /// a block.
    fn add_child(&self,
                 name: Name,
                 reduced_graph_parent: ReducedGraphParent,
                 duplicate_checking_mode: DuplicateCheckingMode,
                 // For printing errors
                 sp: Span)
                 -> Rc<NameBindings> {
        // If this is the immediate descendant of a module, then we add the
        // child name directly. Otherwise, we create or reuse an anonymous
        // module and add the child to that.

        let module_ = reduced_graph_parent.module();

        self.check_for_conflicts_between_external_crates_and_items(&*module_,
                                                                   name,
                                                                   sp);

        // Add or reuse the child.
        let child = module_.children.borrow().get(&name).cloned();
        match child {
            None => {
                let child = Rc::new(NameBindings::new());
                module_.children.borrow_mut().insert(name, child.clone());
                child
            }
            Some(child) => {
                // Enforce the duplicate checking mode:
                //
                // * If we're requesting duplicate module checking, check that
                //   there isn't a module in the module with the same name.
                //
                // * If we're requesting duplicate type checking, check that
                //   there isn't a type in the module with the same name.
                //
                // * If we're requesting duplicate value checking, check that
                //   there isn't a value in the module with the same name.
                //
                // * If we're requesting duplicate type checking and duplicate
                //   value checking, check that there isn't a duplicate type
                //   and a duplicate value with the same name.
                //
                // * If no duplicate checking was requested at all, do
                //   nothing.

                let mut duplicate_type = NoError;
                let ns = match duplicate_checking_mode {
                    ForbidDuplicateModules => {
                        if child.get_module_if_available().is_some() {
                            duplicate_type = ModuleError;
                        }
                        Some(TypeNS)
                    }
                    ForbidDuplicateTypesAndModules => {
                        match child.def_for_namespace(TypeNS) {
                            None => {}
                            Some(_) if child.get_module_if_available()
                                            .map(|m| m.kind.get()) ==
                                       Some(ImplModuleKind) => {}
                            Some(_) => duplicate_type = TypeError
                        }
                        Some(TypeNS)
                    }
                    ForbidDuplicateValues => {
                        if child.defined_in_namespace(ValueNS) {
                            duplicate_type = ValueError;
                        }
                        Some(ValueNS)
                    }
                    ForbidDuplicateTypesAndValues => {
                        let mut n = None;
                        match child.def_for_namespace(TypeNS) {
                            Some(DefMod(_)) | None => {}
                            Some(_) => {
                                n = Some(TypeNS);
                                duplicate_type = TypeError;
                            }
                        };
                        if child.defined_in_namespace(ValueNS) {
                            duplicate_type = ValueError;
                            n = Some(ValueNS);
                        }
                        n
                    }
                    OverwriteDuplicates => None
                };
                if duplicate_type != NoError {
                    // Return an error here by looking up the namespace that
                    // had the duplicate.
                    let ns = ns.unwrap();
                    self.resolve_error(sp,
                        format!("duplicate definition of {} `{}`",
                             namespace_error_to_string(duplicate_type),
                             token::get_name(name)).as_slice());
                    {
                        let r = child.span_for_namespace(ns);
                        for sp in r.iter() {
                            self.session.span_note(*sp,
                                 format!("first definition of {} `{}` here",
                                      namespace_error_to_string(duplicate_type),
                                      token::get_name(name)).as_slice());
                        }
                    }
                }
                child
            }
        }
    }

    fn block_needs_anonymous_module(&mut self, block: &Block) -> bool {
        // If the block has view items, we need an anonymous module.
        if block.view_items.len() > 0 {
            return true;
        }

        // Check each statement.
        for statement in block.stmts.iter() {
            match statement.node {
                StmtDecl(ref declaration, _) => {
                    match declaration.node {
                        DeclItem(_) => {
                            return true;
                        }
                        _ => {
                            // Keep searching.
                        }
                    }
                }
                _ => {
                    // Keep searching.
                }
            }
        }

        // If we found neither view items nor items, we don't need to create
        // an anonymous module.

        return false;
    }

    fn get_parent_link(&mut self, parent: ReducedGraphParent, name: Name)
                           -> ParentLink {
        match parent {
            ModuleReducedGraphParent(module_) => {
                return ModuleParentLink(module_.downgrade(), name);
            }
        }
    }

    /// Constructs the reduced graph for one item.
    fn build_reduced_graph_for_item(&mut self,
                                    item: &Item,
                                    parent: ReducedGraphParent)
                                    -> ReducedGraphParent
    {
        let name = item.ident.name;
        let sp = item.span;
        let is_public = item.vis == ast::Public;
        let modifiers = if is_public { PUBLIC } else { DefModifiers::empty() } | IMPORTABLE;

        match item.node {
            ItemMod(..) => {
                let name_bindings =
                    self.add_child(name, parent.clone(), ForbidDuplicateModules, sp);

                let parent_link = self.get_parent_link(parent, name);
                let def_id = DefId { krate: 0, node: item.id };
                name_bindings.define_module(parent_link,
                                            Some(def_id),
                                            NormalModuleKind,
                                            false,
                                            item.vis == ast::Public,
                                            sp);

                ModuleReducedGraphParent(name_bindings.get_module())
            }

            ItemForeignMod(..) => parent,

            // These items live in the value namespace.
            ItemStatic(_, m, _) => {
                let name_bindings =
                    self.add_child(name, parent.clone(), ForbidDuplicateValues, sp);
                let mutbl = m == ast::MutMutable;

                name_bindings.define_value
                    (DefStatic(local_def(item.id), mutbl), sp, modifiers);
                parent
            }
            ItemConst(_, _) => {
                self.add_child(name, parent.clone(), ForbidDuplicateValues, sp)
                    .define_value(DefConst(local_def(item.id)),
                                  sp, modifiers);
                parent
            }
            ItemFn(_, _, _, _, _) => {
                let name_bindings =
                    self.add_child(name, parent.clone(), ForbidDuplicateValues, sp);

                let def = DefFn(local_def(item.id), false);
                name_bindings.define_value(def, sp, modifiers);
                parent
            }

            // These items live in the type namespace.
            ItemTy(..) => {
                let name_bindings =
                    self.add_child(name,
                                   parent.clone(),
                                   ForbidDuplicateTypesAndModules,
                                   sp);

                name_bindings.define_type
                    (DefTy(local_def(item.id), false), sp, modifiers);
                parent
            }

            ItemEnum(ref enum_definition, _) => {
                let name_bindings =
                    self.add_child(name,
                                   parent.clone(),
                                   ForbidDuplicateTypesAndModules,
                                   sp);

                name_bindings.define_type
                    (DefTy(local_def(item.id), true), sp, modifiers);

                let parent_link = self.get_parent_link(parent.clone(), name);
                // We want to make sure the module type is EnumModuleKind
                // even if there's already an ImplModuleKind module defined,
                // since that's how we prevent duplicate enum definitions
                name_bindings.set_module_kind(parent_link,
                                              Some(local_def(item.id)),
                                              EnumModuleKind,
                                              false,
                                              is_public,
                                              sp);

                for variant in (*enum_definition).variants.iter() {
                    self.build_reduced_graph_for_variant(
                        &**variant,
                        local_def(item.id),
                        ModuleReducedGraphParent(name_bindings.get_module()));
                }
                parent
            }

            // These items live in both the type and value namespaces.
            ItemStruct(ref struct_def, _) => {
                // Adding to both Type and Value namespaces or just Type?
                let (forbid, ctor_id) = match struct_def.ctor_id {
                    Some(ctor_id)   => (ForbidDuplicateTypesAndValues, Some(ctor_id)),
                    None            => (ForbidDuplicateTypesAndModules, None)
                };

                let name_bindings = self.add_child(name, parent.clone(), forbid, sp);

                // Define a name in the type namespace.
                name_bindings.define_type(DefTy(local_def(item.id), false), sp, modifiers);

                // If this is a newtype or unit-like struct, define a name
                // in the value namespace as well
                match ctor_id {
                    Some(cid) => {
                        name_bindings.define_value(DefStruct(local_def(cid)),
                                                   sp, modifiers);
                    }
                    None => {}
                }

                // Record the def ID and fields of this struct.
                let named_fields = struct_def.fields.iter().filter_map(|f| {
                    match f.node.kind {
                        NamedField(ident, _) => Some(ident.name),
                        UnnamedField(_) => None
                    }
                }).collect();
                self.structs.insert(local_def(item.id), named_fields);

                parent
            }

            ItemImpl(_, None, ref ty, ref impl_items) => {
                // If this implements an anonymous trait, then add all the
                // methods within to a new module, if the type was defined
                // within this module.

                let mod_name = match ty.node {
                    TyPath(ref path, _) if path.segments.len() == 1 => {
                        // FIXME(18446) we should distinguish between the name of
                        // a trait and the name of an impl of that trait.
                        Some(path.segments.last().unwrap().identifier.name)
                    }
                    TyObjectSum(ref lhs_ty, _) => {
                        match lhs_ty.node {
                            TyPath(ref path, _) if path.segments.len() == 1 => {
                                Some(path.segments.last().unwrap().identifier.name)
                            }
                            _ => {
                                None
                            }
                        }
                    }
                    _ => {
                        None
                    }
                };

                match mod_name {
                    None => {
                        self.resolve_error(ty.span,
                                           "inherent implementations may \
                                            only be implemented in the same \
                                            module as the type they are \
                                            implemented for")
                    }
                    Some(mod_name) => {
                        // Create the module and add all methods.
                        let parent_opt = parent.module().children.borrow()
                            .get(&mod_name).cloned();
                        let new_parent = match parent_opt {
                            // It already exists
                            Some(ref child) if child.get_module_if_available()
                                .is_some() &&
                                (child.get_module().kind.get() == ImplModuleKind ||
                                 child.get_module().kind.get() == TraitModuleKind) => {
                                    ModuleReducedGraphParent(child.get_module())
                                }
                            Some(ref child) if child.get_module_if_available()
                                .is_some() &&
                                child.get_module().kind.get() ==
                                EnumModuleKind => {
                                    ModuleReducedGraphParent(child.get_module())
                                }
                            // Create the module
                            _ => {
                                let name_bindings =
                                    self.add_child(mod_name,
                                                   parent.clone(),
                                                   ForbidDuplicateModules,
                                                   sp);

                                let parent_link =
                                    self.get_parent_link(parent.clone(), name);
                                let def_id = local_def(item.id);
                                let ns = TypeNS;
                                let is_public =
                                    !name_bindings.defined_in_namespace(ns) ||
                                    name_bindings.defined_in_public_namespace(ns);

                                name_bindings.define_module(parent_link,
                                                            Some(def_id),
                                                            ImplModuleKind,
                                                            false,
                                                            is_public,
                                                            sp);

                                ModuleReducedGraphParent(
                                    name_bindings.get_module())
                            }
                        };

                        // For each implementation item...
                        for impl_item in impl_items.iter() {
                            match *impl_item {
                                MethodImplItem(ref method) => {
                                    // Add the method to the module.
                                    let name = method.pe_ident().name;
                                    let method_name_bindings =
                                        self.add_child(name,
                                                       new_parent.clone(),
                                                       ForbidDuplicateValues,
                                                       method.span);
                                    let def = match method.pe_explicit_self()
                                        .node {
                                            SelfStatic => {
                                                // Static methods become
                                                // `DefStaticMethod`s.
                                                DefStaticMethod(local_def(method.id),
                                                                FromImpl(local_def(item.id)))
                                            }
                                            _ => {
                                                // Non-static methods become
                                                // `DefMethod`s.
                                                DefMethod(local_def(method.id),
                                                          None,
                                                          FromImpl(local_def(item.id)))
                                            }
                                        };

                                    // NB: not IMPORTABLE
                                    let modifiers = if method.pe_vis() == ast::Public {
                                        PUBLIC
                                    } else {
                                        DefModifiers::empty()
                                    };
                                    method_name_bindings.define_value(
                                        def,
                                        method.span,
                                        modifiers);
                                }
                                TypeImplItem(ref typedef) => {
                                    // Add the typedef to the module.
                                    let name = typedef.ident.name;
                                    let typedef_name_bindings =
                                        self.add_child(
                                            name,
                                            new_parent.clone(),
                                            ForbidDuplicateTypesAndModules,
                                            typedef.span);
                                    let def = DefAssociatedTy(local_def(
                                        typedef.id));
                                    // NB: not IMPORTABLE
                                    let modifiers = if typedef.vis == ast::Public {
                                        PUBLIC
                                    } else {
                                        DefModifiers::empty()
                                    };
                                    typedef_name_bindings.define_type(
                                        def,
                                        typedef.span,
                                        modifiers);
                                }
                            }
                        }
                    }
                }

                parent
            }

            ItemImpl(_, Some(_), _, _) => parent,

            ItemTrait(_, _, _, ref methods) => {
                let name_bindings =
                    self.add_child(name,
                                   parent.clone(),
                                   ForbidDuplicateTypesAndModules,
                                   sp);

                // Add all the methods within to a new module.
                let parent_link = self.get_parent_link(parent.clone(), name);
                name_bindings.define_module(parent_link,
                                            Some(local_def(item.id)),
                                            TraitModuleKind,
                                            false,
                                            item.vis == ast::Public,
                                            sp);
                let module_parent = ModuleReducedGraphParent(name_bindings.
                                                             get_module());

                let def_id = local_def(item.id);

                // Add the names of all the methods to the trait info.
                for method in methods.iter() {
                    let (name, kind) = match *method {
                        ast::RequiredMethod(_) |
                        ast::ProvidedMethod(_) => {
                            let ty_m =
                                ast_util::trait_item_to_ty_method(method);

                            let name = ty_m.ident.name;

                            // Add it as a name in the trait module.
                            let (def, static_flag) = match ty_m.explicit_self
                                                               .node {
                                SelfStatic => {
                                    // Static methods become `DefStaticMethod`s.
                                    (DefStaticMethod(
                                            local_def(ty_m.id),
                                            FromTrait(local_def(item.id))),
                                     StaticMethodTraitItemKind)
                                }
                                _ => {
                                    // Non-static methods become `DefMethod`s.
                                    (DefMethod(local_def(ty_m.id),
                                               Some(local_def(item.id)),
                                               FromTrait(local_def(item.id))),
                                     NonstaticMethodTraitItemKind)
                                }
                            };

                            let method_name_bindings =
                                self.add_child(name,
                                               module_parent.clone(),
                                               ForbidDuplicateTypesAndValues,
                                               ty_m.span);
                            // NB: not IMPORTABLE
                            method_name_bindings.define_value(def,
                                                              ty_m.span,
                                                              PUBLIC);

                            (name, static_flag)
                        }
                        ast::TypeTraitItem(ref associated_type) => {
                            let def = DefAssociatedTy(local_def(
                                    associated_type.ty_param.id));

                            let name_bindings =
                                self.add_child(associated_type.ty_param.ident.name,
                                               module_parent.clone(),
                                               ForbidDuplicateTypesAndValues,
                                               associated_type.ty_param.span);
                            // NB: not IMPORTABLE
                            name_bindings.define_type(def,
                                                      associated_type.ty_param.span,
                                                      PUBLIC);

                            (associated_type.ty_param.ident.name, TypeTraitItemKind)
                        }
                    };

                    self.trait_item_map.insert((name, def_id), kind);
                }

                name_bindings.define_type(DefTrait(def_id), sp, modifiers);
                parent
            }
            ItemMac(..) => parent
        }
    }

    // Constructs the reduced graph for one variant. Variants exist in the
    // type and value namespaces.
    fn build_reduced_graph_for_variant(&mut self,
                                       variant: &Variant,
                                       item_id: DefId,
                                       parent: ReducedGraphParent) {
        let name = variant.node.name.name;
        let is_exported = match variant.node.kind {
            TupleVariantKind(_) => false,
            StructVariantKind(_) => {
                // Not adding fields for variants as they are not accessed with a self receiver
                self.structs.insert(local_def(variant.node.id), Vec::new());
                true
            }
        };

        let child = self.add_child(name, parent,
                                   ForbidDuplicateTypesAndValues,
                                   variant.span);
        // variants are always treated as importable to allow them to be glob
        // used
        child.define_value(DefVariant(item_id,
                                      local_def(variant.node.id), is_exported),
                           variant.span, PUBLIC | IMPORTABLE);
        child.define_type(DefVariant(item_id,
                                     local_def(variant.node.id), is_exported),
                          variant.span, PUBLIC | IMPORTABLE);
    }

    /// Constructs the reduced graph for one 'view item'. View items consist
    /// of imports and use directives.
    fn build_reduced_graph_for_view_item(&mut self, view_item: &ViewItem,
                                         parent: ReducedGraphParent) {
        match view_item.node {
            ViewItemUse(ref view_path) => {
                // Extract and intern the module part of the path. For
                // globs and lists, the path is found directly in the AST;
                // for simple paths we have to munge the path a little.
                let module_path = match view_path.node {
                    ViewPathSimple(_, ref full_path, _) => {
                        full_path.segments
                            .init()
                            .iter().map(|ident| ident.identifier.name)
                            .collect()
                    }

                    ViewPathGlob(ref module_ident_path, _) |
                    ViewPathList(ref module_ident_path, _, _) => {
                        module_ident_path.segments
                            .iter().map(|ident| ident.identifier.name).collect()
                    }
                };

                // Build up the import directives.
                let module_ = parent.module();
                let is_public = view_item.vis == ast::Public;
                let shadowable =
                    view_item.attrs
                             .iter()
                             .any(|attr| {
                                 attr.name() == token::get_name(
                                    special_idents::prelude_import.name)
                             });

                match view_path.node {
                    ViewPathSimple(binding, ref full_path, id) => {
                        let source_name =
                            full_path.segments.last().unwrap().identifier.name;
                        if token::get_name(source_name).get() == "mod" {
                            self.resolve_error(view_path.span,
                                "`mod` imports are only allowed within a { } list");
                        }

                        let subclass = SingleImport(binding.name,
                                                    source_name);
                        self.build_import_directive(&*module_,
                                                    module_path,
                                                    subclass,
                                                    view_path.span,
                                                    id,
                                                    is_public,
                                                    shadowable);
                    }
                    ViewPathList(_, ref source_items, _) => {
                        // Make sure there's at most one `mod` import in the list.
                        let mod_spans = source_items.iter().filter_map(|item| match item.node {
                            PathListMod { .. } => Some(item.span),
                            _ => None
                        }).collect::<Vec<Span>>();
                        if mod_spans.len() > 1 {
                            self.resolve_error(mod_spans[0],
                                "`mod` import can only appear once in the list");
                            for other_span in mod_spans.iter().skip(1) {
                                self.session.span_note(*other_span,
                                    "another `mod` import appears here");
                            }
                        }

                        for source_item in source_items.iter() {
                            let (module_path, name) = match source_item.node {
                                PathListIdent { name, .. } =>
                                    (module_path.clone(), name.name),
                                PathListMod { .. } => {
                                    let name = match module_path.last() {
                                        Some(name) => *name,
                                        None => {
                                            self.resolve_error(source_item.span,
                                                "`mod` import can only appear in an import list \
                                                 with a non-empty prefix");
                                            continue;
                                        }
                                    };
                                    let module_path = module_path.init();
                                    (module_path.to_vec(), name)
                                }
                            };
                            self.build_import_directive(
                                &*module_,
                                module_path,
                                SingleImport(name, name),
                                source_item.span,
                                source_item.node.id(),
                                is_public,
                                shadowable);
                        }
                    }
                    ViewPathGlob(_, id) => {
                        self.build_import_directive(&*module_,
                                                    module_path,
                                                    GlobImport,
                                                    view_path.span,
                                                    id,
                                                    is_public,
                                                    shadowable);
                    }
                }
            }

            ViewItemExternCrate(name, _, node_id) => {
                // n.b. we don't need to look at the path option here, because cstore already did
                for &crate_id in self.session.cstore
                                     .find_extern_mod_stmt_cnum(node_id).iter() {
                    let def_id = DefId { krate: crate_id, node: 0 };
                    self.external_exports.insert(def_id);
                    let parent_link =
                        ModuleParentLink(parent.module().downgrade(), name.name);
                    let external_module = Rc::new(Module::new(parent_link,
                                                              Some(def_id),
                                                              NormalModuleKind,
                                                              false,
                                                              true));
                    debug!("(build reduced graph for item) found extern `{}`",
                            self.module_to_string(&*external_module));
                    self.check_for_conflicts_between_external_crates(
                        &*parent.module(),
                        name.name,
                        view_item.span);
                    parent.module().external_module_children.borrow_mut()
                                   .insert(name.name, external_module.clone());
                    self.build_reduced_graph_for_external_crate(external_module);
                }
            }
        }
    }

    /// Constructs the reduced graph for one foreign item.
    fn build_reduced_graph_for_foreign_item(&mut self,
                                            foreign_item: &ForeignItem,
                                            parent: ReducedGraphParent,
                                            f: |&mut Resolver|) {
        let name = foreign_item.ident.name;
        let is_public = foreign_item.vis == ast::Public;
        let modifiers = if is_public { PUBLIC } else { DefModifiers::empty() } | IMPORTABLE;
        let name_bindings =
            self.add_child(name, parent, ForbidDuplicateValues,
                           foreign_item.span);

        match foreign_item.node {
            ForeignItemFn(_, ref generics) => {
                let def = DefFn(local_def(foreign_item.id), false);
                name_bindings.define_value(def, foreign_item.span, modifiers);

                self.with_type_parameter_rib(
                    HasTypeParameters(generics,
                                      FnSpace,
                                      foreign_item.id,
                                      NormalRibKind),
                    f);
            }
            ForeignItemStatic(_, m) => {
                let def = DefStatic(local_def(foreign_item.id), m);
                name_bindings.define_value(def, foreign_item.span, modifiers);

                f(self)
            }
        }
    }

    fn build_reduced_graph_for_block(&mut self,
                                         block: &Block,
                                         parent: ReducedGraphParent)
                                            -> ReducedGraphParent
    {
        if self.block_needs_anonymous_module(block) {
            let block_id = block.id;

            debug!("(building reduced graph for block) creating a new \
                    anonymous module for block {}",
                   block_id);

            let parent_module = parent.module();
            let new_module = Rc::new(Module::new(
                BlockParentLink(parent_module.downgrade(), block_id),
                None,
                AnonymousModuleKind,
                false,
                false));
            parent_module.anonymous_children.borrow_mut()
                         .insert(block_id, new_module.clone());
            ModuleReducedGraphParent(new_module)
        } else {
            parent
        }
    }

    fn handle_external_def(&mut self,
                           def: Def,
                           vis: Visibility,
                           child_name_bindings: &NameBindings,
                           final_ident: &str,
                           name: Name,
                           new_parent: ReducedGraphParent) {
        debug!("(building reduced graph for \
                external crate) building external def, priv {}",
               vis);
        let is_public = vis == ast::Public;
        let modifiers = if is_public { PUBLIC } else { DefModifiers::empty() } | IMPORTABLE;
        let is_exported = is_public && match new_parent {
            ModuleReducedGraphParent(ref module) => {
                match module.def_id.get() {
                    None => true,
                    Some(did) => self.external_exports.contains(&did)
                }
            }
        };
        if is_exported {
            self.external_exports.insert(def.def_id());
        }

        let kind = match def {
            DefTy(_, true) => EnumModuleKind,
            DefStruct(..) | DefTy(..) => ImplModuleKind,
            _ => NormalModuleKind
        };

        match def {
          DefMod(def_id) | DefForeignMod(def_id) | DefStruct(def_id) |
          DefTy(def_id, _) => {
            let type_def = child_name_bindings.type_def.borrow().clone();
            match type_def {
              Some(TypeNsDef { module_def: Some(module_def), .. }) => {
                debug!("(building reduced graph for external crate) \
                        already created module");
                module_def.def_id.set(Some(def_id));
              }
              Some(_) | None => {
                debug!("(building reduced graph for \
                        external crate) building module \
                        {}", final_ident);
                let parent_link = self.get_parent_link(new_parent.clone(), name);

                child_name_bindings.define_module(parent_link,
                                                  Some(def_id),
                                                  kind,
                                                  true,
                                                  is_public,
                                                  DUMMY_SP);
              }
            }
          }
          _ => {}
        }

        match def {
          DefMod(_) | DefForeignMod(_) => {}
          DefVariant(_, variant_id, is_struct) => {
              debug!("(building reduced graph for external crate) building \
                      variant {}",
                      final_ident);
              // variants are always treated as importable to allow them to be
              // glob used
              let modifiers = PUBLIC | IMPORTABLE;
              if is_struct {
                  child_name_bindings.define_type(def, DUMMY_SP, modifiers);
                  // Not adding fields for variants as they are not accessed with a self receiver
                  self.structs.insert(variant_id, Vec::new());
              } else {
                  child_name_bindings.define_value(def, DUMMY_SP, modifiers);
              }
          }
          DefFn(ctor_id, true) => {
            child_name_bindings.define_value(
                csearch::get_tuple_struct_definition_if_ctor(&self.session.cstore, ctor_id)
                    .map_or(def, |_| DefStruct(ctor_id)), DUMMY_SP, modifiers);
          }
          DefFn(..) | DefStaticMethod(..) | DefStatic(..) | DefConst(..) | DefMethod(..) => {
            debug!("(building reduced graph for external \
                    crate) building value (fn/static) {}", final_ident);
            // impl methods have already been defined with the correct importability modifier
            let mut modifiers = match *child_name_bindings.value_def.borrow() {
                Some(ref def) => (modifiers & !IMPORTABLE) | (def.modifiers & IMPORTABLE),
                None => modifiers
            };
            if new_parent.module().kind.get() != NormalModuleKind {
                modifiers = modifiers & !IMPORTABLE;
            }
            child_name_bindings.define_value(def, DUMMY_SP, modifiers);
          }
          DefTrait(def_id) => {
              debug!("(building reduced graph for external \
                      crate) building type {}", final_ident);

              // If this is a trait, add all the trait item names to the trait
              // info.

              let trait_item_def_ids =
                csearch::get_trait_item_def_ids(&self.session.cstore, def_id);
              for trait_item_def_id in trait_item_def_ids.iter() {
                  let (trait_item_name, trait_item_kind) =
                      csearch::get_trait_item_name_and_kind(
                          &self.session.cstore,
                          trait_item_def_id.def_id());

                  debug!("(building reduced graph for external crate) ... \
                          adding trait item '{}'",
                         token::get_name(trait_item_name));

                  self.trait_item_map.insert((trait_item_name, def_id), trait_item_kind);

                  if is_exported {
                      self.external_exports
                          .insert(trait_item_def_id.def_id());
                  }
              }

              child_name_bindings.define_type(def, DUMMY_SP, modifiers);

              // Define a module if necessary.
              let parent_link = self.get_parent_link(new_parent, name);
              child_name_bindings.set_module_kind(parent_link,
                                                  Some(def_id),
                                                  TraitModuleKind,
                                                  true,
                                                  is_public,
                                                  DUMMY_SP)
          }
          DefTy(..) | DefAssociatedTy(..) => {
              debug!("(building reduced graph for external \
                      crate) building type {}", final_ident);

              child_name_bindings.define_type(def, DUMMY_SP, modifiers);
          }
          DefStruct(def_id) => {
            debug!("(building reduced graph for external \
                    crate) building type and value for {}",
                   final_ident);
            child_name_bindings.define_type(def, DUMMY_SP, modifiers);
            let fields = csearch::get_struct_fields(&self.session.cstore, def_id).iter().map(|f| {
                f.name
            }).collect::<Vec<_>>();

            if fields.len() == 0 {
                child_name_bindings.define_value(def, DUMMY_SP, modifiers);
            }

            // Record the def ID and fields of this struct.
            self.structs.insert(def_id, fields);
          }
          DefLocal(..) | DefPrimTy(..) | DefTyParam(..) |
          DefUse(..) | DefUpvar(..) | DefRegion(..) |
          DefTyParamBinder(..) | DefLabel(..) | DefSelfTy(..) => {
            panic!("didn't expect `{}`", def);
          }
        }
    }

    /// Builds the reduced graph for a single item in an external crate.
    fn build_reduced_graph_for_external_crate_def(&mut self,
                                                  root: Rc<Module>,
                                                  def_like: DefLike,
                                                  name: Name,
                                                  visibility: Visibility) {
        match def_like {
            DlDef(def) => {
                // Add the new child item, if necessary.
                match def {
                    DefForeignMod(def_id) => {
                        // Foreign modules have no names. Recur and populate
                        // eagerly.
                        csearch::each_child_of_item(&self.session.cstore,
                                                    def_id,
                                                    |def_like,
                                                     child_name,
                                                     vis| {
                            self.build_reduced_graph_for_external_crate_def(
                                root.clone(),
                                def_like,
                                child_name,
                                vis)
                        });
                    }
                    _ => {
                        let child_name_bindings =
                            self.add_child(name,
                                           ModuleReducedGraphParent(root.clone()),
                                           OverwriteDuplicates,
                                           DUMMY_SP);

                        self.handle_external_def(def,
                                                 visibility,
                                                 &*child_name_bindings,
                                                 token::get_name(name).get(),
                                                 name,
                                                 ModuleReducedGraphParent(root));
                    }
                }
            }
            DlImpl(def) => {
                match csearch::get_type_name_if_impl(&self.session.cstore, def) {
                    None => {}
                    Some(final_name) => {
                        let methods_opt =
                            csearch::get_methods_if_impl(&self.session.cstore, def);
                        match methods_opt {
                            Some(ref methods) if
                                methods.len() >= 1 => {
                                debug!("(building reduced graph for \
                                        external crate) processing \
                                        static methods for type name {}",
                                        token::get_name(final_name));

                                let child_name_bindings =
                                    self.add_child(
                                        final_name,
                                        ModuleReducedGraphParent(root.clone()),
                                        OverwriteDuplicates,
                                        DUMMY_SP);

                                // Process the static methods. First,
                                // create the module.
                                let type_module;
                                let type_def = child_name_bindings.type_def.borrow().clone();
                                match type_def {
                                    Some(TypeNsDef {
                                        module_def: Some(module_def),
                                        ..
                                    }) => {
                                        // We already have a module. This
                                        // is OK.
                                        type_module = module_def;

                                        // Mark it as an impl module if
                                        // necessary.
                                        type_module.kind.set(ImplModuleKind);
                                    }
                                    Some(_) | None => {
                                        let parent_link =
                                            self.get_parent_link(ModuleReducedGraphParent(root),
                                                                 final_name);
                                        child_name_bindings.define_module(
                                            parent_link,
                                            Some(def),
                                            ImplModuleKind,
                                            true,
                                            true,
                                            DUMMY_SP);
                                        type_module =
                                            child_name_bindings.
                                                get_module();
                                    }
                                }

                                // Add each static method to the module.
                                let new_parent =
                                    ModuleReducedGraphParent(type_module);
                                for method_info in methods.iter() {
                                    let name = method_info.name;
                                    debug!("(building reduced graph for \
                                             external crate) creating \
                                             static method '{}'",
                                           token::get_name(name));

                                    let method_name_bindings =
                                        self.add_child(name,
                                                       new_parent.clone(),
                                                       OverwriteDuplicates,
                                                       DUMMY_SP);
                                    let def = DefFn(method_info.def_id, false);

                                    // NB: not IMPORTABLE
                                    let modifiers = if visibility == ast::Public {
                                        PUBLIC
                                    } else {
                                        DefModifiers::empty()
                                    };
                                    method_name_bindings.define_value(
                                        def, DUMMY_SP, modifiers);
                                }
                            }

                            // Otherwise, do nothing.
                            Some(_) | None => {}
                        }
                    }
                }
            }
            DlField => {
                debug!("(building reduced graph for external crate) \
                        ignoring field");
            }
        }
    }

    /// Builds the reduced graph rooted at the given external module.
    fn populate_external_module(&mut self, module: Rc<Module>) {
        debug!("(populating external module) attempting to populate {}",
               self.module_to_string(&*module));

        let def_id = match module.def_id.get() {
            None => {
                debug!("(populating external module) ... no def ID!");
                return
            }
            Some(def_id) => def_id,
        };

        csearch::each_child_of_item(&self.session.cstore,
                                    def_id,
                                    |def_like, child_name, visibility| {
            debug!("(populating external module) ... found ident: {}",
                   token::get_name(child_name));
            self.build_reduced_graph_for_external_crate_def(module.clone(),
                                                            def_like,
                                                            child_name,
                                                            visibility)
        });
        module.populated.set(true)
    }

    /// Ensures that the reduced graph rooted at the given external module
    /// is built, building it if it is not.
    fn populate_module_if_necessary(&mut self, module: &Rc<Module>) {
        if !module.populated.get() {
            self.populate_external_module(module.clone())
        }
        assert!(module.populated.get())
    }

    /// Builds the reduced graph rooted at the 'use' directive for an external
    /// crate.
    fn build_reduced_graph_for_external_crate(&mut self, root: Rc<Module>) {
        csearch::each_top_level_item_of_crate(&self.session.cstore,
                                              root.def_id
                                                  .get()
                                                  .unwrap()
                                                  .krate,
                                              |def_like, name, visibility| {
            self.build_reduced_graph_for_external_crate_def(root.clone(),
                                                            def_like,
                                                            name,
                                                            visibility)
        });
    }

    /// Creates and adds an import directive to the given module.
    fn build_import_directive(&mut self,
                              module_: &Module,
                              module_path: Vec<Name>,
                              subclass: ImportDirectiveSubclass,
                              span: Span,
                              id: NodeId,
                              is_public: bool,
                              shadowable: bool) {
        module_.imports.borrow_mut().push(ImportDirective::new(module_path,
                                                               subclass,
                                                               span,
                                                               id,
                                                               is_public,
                                                               shadowable));
        self.unresolved_imports += 1;
        // Bump the reference count on the name. Or, if this is a glob, set
        // the appropriate flag.

        match subclass {
            SingleImport(target, _) => {
                debug!("(building import directive) building import \
                        directive: {}::{}",
                       self.names_to_string(module_.imports.borrow().last().unwrap()
                                                 .module_path.as_slice()),
                       token::get_name(target));

                let mut import_resolutions = module_.import_resolutions
                                                    .borrow_mut();
                match import_resolutions.get_mut(&target) {
                    Some(resolution) => {
                        debug!("(building import directive) bumping \
                                reference");
                        resolution.outstanding_references += 1;

                        // the source of this name is different now
                        resolution.type_id = id;
                        resolution.value_id = id;
                        resolution.is_public = is_public;
                        return;
                    }
                    None => {}
                }
                debug!("(building import directive) creating new");
                let mut resolution = ImportResolution::new(id, is_public);
                resolution.outstanding_references = 1;
                import_resolutions.insert(target, resolution);
            }
            GlobImport => {
                // Set the glob flag. This tells us that we don't know the
                // module's exports ahead of time.

                module_.glob_count.set(module_.glob_count.get() + 1);
            }
        }
    }

    // Import resolution
    //
    // This is a fixed-point algorithm. We resolve imports until our efforts
    // are stymied by an unresolved import; then we bail out of the current
    // module and continue. We terminate successfully once no more imports
    // remain or unsuccessfully when no forward progress in resolving imports
    // is made.

    /// Resolves all imports for the crate. This method performs the fixed-
    /// point iteration.
    fn resolve_imports(&mut self) {
        let mut i = 0u;
        let mut prev_unresolved_imports = 0;
        loop {
            debug!("(resolving imports) iteration {}, {} imports left",
                   i, self.unresolved_imports);

            let module_root = self.graph_root.get_module();
            self.resolve_imports_for_module_subtree(module_root.clone());

            if self.unresolved_imports == 0 {
                debug!("(resolving imports) success");
                break;
            }

            if self.unresolved_imports == prev_unresolved_imports {
                self.report_unresolved_imports(module_root);
                break;
            }

            i += 1;
            prev_unresolved_imports = self.unresolved_imports;
        }
    }

    /// Attempts to resolve imports for the given module and all of its
    /// submodules.
    fn resolve_imports_for_module_subtree(&mut self, module_: Rc<Module>) {
        debug!("(resolving imports for module subtree) resolving {}",
               self.module_to_string(&*module_));
        let orig_module = replace(&mut self.current_module, module_.clone());
        self.resolve_imports_for_module(module_.clone());
        self.current_module = orig_module;

        self.populate_module_if_necessary(&module_);
        for (_, child_node) in module_.children.borrow().iter() {
            match child_node.get_module_if_available() {
                None => {
                    // Nothing to do.
                }
                Some(child_module) => {
                    self.resolve_imports_for_module_subtree(child_module);
                }
            }
        }

        for (_, child_module) in module_.anonymous_children.borrow().iter() {
            self.resolve_imports_for_module_subtree(child_module.clone());
        }
    }

    /// Attempts to resolve imports for the given module only.
    fn resolve_imports_for_module(&mut self, module: Rc<Module>) {
        if module.all_imports_resolved() {
            debug!("(resolving imports for module) all imports resolved for \
                   {}",
                   self.module_to_string(&*module));
            return;
        }

        let imports = module.imports.borrow();
        let import_count = imports.len();
        while module.resolved_import_count.get() < import_count {
            let import_index = module.resolved_import_count.get();
            let import_directive = &(*imports)[import_index];
            match self.resolve_import_for_module(module.clone(),
                                                 import_directive) {
                Failed(err) => {
                    let (span, help) = match err {
                        Some((span, msg)) => (span, format!(". {}", msg)),
                        None => (import_directive.span, String::new())
                    };
                    let msg = format!("unresolved import `{}`{}",
                                      self.import_path_to_string(
                                          import_directive.module_path
                                                          .as_slice(),
                                          import_directive.subclass),
                                      help);
                    self.resolve_error(span, msg.as_slice());
                }
                Indeterminate => break, // Bail out. We'll come around next time.
                Success(()) => () // Good. Continue.
            }

            module.resolved_import_count
                  .set(module.resolved_import_count.get() + 1);
        }
    }

    fn names_to_string(&self, names: &[Name]) -> String {
        let mut first = true;
        let mut result = String::new();
        for name in names.iter() {
            if first {
                first = false
            } else {
                result.push_str("::")
            }
            result.push_str(token::get_name(*name).get());
        };
        result
    }

    fn path_names_to_string(&self, path: &Path) -> String {
        let names: Vec<ast::Name> = path.segments
                                        .iter()
                                        .map(|seg| seg.identifier.name)
                                        .collect();
        self.names_to_string(names.as_slice())
    }

    fn import_directive_subclass_to_string(&mut self,
                                        subclass: ImportDirectiveSubclass)
                                        -> String {
        match subclass {
            SingleImport(_, source) => {
                token::get_name(source).get().to_string()
            }
            GlobImport => "*".to_string()
        }
    }

    fn import_path_to_string(&mut self,
                          names: &[Name],
                          subclass: ImportDirectiveSubclass)
                          -> String {
        if names.is_empty() {
            self.import_directive_subclass_to_string(subclass)
        } else {
            (format!("{}::{}",
                     self.names_to_string(names),
                     self.import_directive_subclass_to_string(
                         subclass))).to_string()
        }
    }

    /// Attempts to resolve the given import. The return value indicates
    /// failure if we're certain the name does not exist, indeterminate if we
    /// don't know whether the name exists at the moment due to other
    /// currently-unresolved imports, or success if we know the name exists.
    /// If successful, the resolved bindings are written into the module.
    fn resolve_import_for_module(&mut self,
                                 module_: Rc<Module>,
                                 import_directive: &ImportDirective)
                                 -> ResolveResult<()> {
        let mut resolution_result = Failed(None);
        let module_path = &import_directive.module_path;

        debug!("(resolving import for module) resolving import `{}::...` in \
                `{}`",
               self.names_to_string(module_path.as_slice()),
               self.module_to_string(&*module_));

        // First, resolve the module path for the directive, if necessary.
        let container = if module_path.len() == 0 {
            // Use the crate root.
            Some((self.graph_root.get_module(), LastMod(AllPublic)))
        } else {
            match self.resolve_module_path(module_.clone(),
                                           module_path.as_slice(),
                                           DontUseLexicalScope,
                                           import_directive.span,
                                           ImportSearch) {
                Failed(err) => {
                    resolution_result = Failed(err);
                    None
                },
                Indeterminate => {
                    resolution_result = Indeterminate;
                    None
                }
                Success(container) => Some(container),
            }
        };

        match container {
            None => {}
            Some((containing_module, lp)) => {
                // We found the module that the target is contained
                // within. Attempt to resolve the import within it.

                match import_directive.subclass {
                    SingleImport(target, source) => {
                        resolution_result =
                            self.resolve_single_import(&*module_,
                                                       containing_module,
                                                       target,
                                                       source,
                                                       import_directive,
                                                       lp);
                    }
                    GlobImport => {
                        resolution_result =
                            self.resolve_glob_import(&*module_,
                                                     containing_module,
                                                     import_directive,
                                                     lp);
                    }
                }
            }
        }

        // Decrement the count of unresolved imports.
        match resolution_result {
            Success(()) => {
                assert!(self.unresolved_imports >= 1);
                self.unresolved_imports -= 1;
            }
            _ => {
                // Nothing to do here; just return the error.
            }
        }

        // Decrement the count of unresolved globs if necessary. But only if
        // the resolution result is indeterminate -- otherwise we'll stop
        // processing imports here. (See the loop in
        // resolve_imports_for_module.)

        if !resolution_result.indeterminate() {
            match import_directive.subclass {
                GlobImport => {
                    assert!(module_.glob_count.get() >= 1);
                    module_.glob_count.set(module_.glob_count.get() - 1);
                }
                SingleImport(..) => {
                    // Ignore.
                }
            }
        }

        return resolution_result;
    }

    fn create_name_bindings_from_module(module: Rc<Module>) -> NameBindings {
        NameBindings {
            type_def: RefCell::new(Some(TypeNsDef {
                modifiers: IMPORTABLE,
                module_def: Some(module),
                type_def: None,
                type_span: None
            })),
            value_def: RefCell::new(None),
        }
    }

    fn resolve_single_import(&mut self,
                             module_: &Module,
                             containing_module: Rc<Module>,
                             target: Name,
                             source: Name,
                             directive: &ImportDirective,
                             lp: LastPrivate)
                                 -> ResolveResult<()> {
        debug!("(resolving single import) resolving `{}` = `{}::{}` from \
                `{}` id {}, last private {}",
               token::get_name(target),
               self.module_to_string(&*containing_module),
               token::get_name(source),
               self.module_to_string(module_),
               directive.id,
               lp);

        let lp = match lp {
            LastMod(lp) => lp,
            LastImport {..} => {
                self.session
                    .span_bug(directive.span,
                              "not expecting Import here, must be LastMod")
            }
        };

        // We need to resolve both namespaces for this to succeed.
        //

        let mut value_result = UnknownResult;
        let mut type_result = UnknownResult;

        // Search for direct children of the containing module.
        self.populate_module_if_necessary(&containing_module);

        match containing_module.children.borrow().get(&source) {
            None => {
                // Continue.
            }
            Some(ref child_name_bindings) => {
                if child_name_bindings.defined_in_namespace(ValueNS) {
                    debug!("(resolving single import) found value binding");
                    value_result = BoundResult(containing_module.clone(),
                                               (*child_name_bindings).clone());
                }
                if child_name_bindings.defined_in_namespace(TypeNS) {
                    debug!("(resolving single import) found type binding");
                    type_result = BoundResult(containing_module.clone(),
                                              (*child_name_bindings).clone());
                }
            }
        }

        // Unless we managed to find a result in both namespaces (unlikely),
        // search imports as well.
        let mut value_used_reexport = false;
        let mut type_used_reexport = false;
        match (value_result.clone(), type_result.clone()) {
            (BoundResult(..), BoundResult(..)) => {} // Continue.
            _ => {
                // If there is an unresolved glob at this point in the
                // containing module, bail out. We don't know enough to be
                // able to resolve this import.

                if containing_module.glob_count.get() > 0 {
                    debug!("(resolving single import) unresolved glob; \
                            bailing out");
                    return Indeterminate;
                }

                // Now search the exported imports within the containing module.
                match containing_module.import_resolutions.borrow().get(&source) {
                    None => {
                        debug!("(resolving single import) no import");
                        // The containing module definitely doesn't have an
                        // exported import with the name in question. We can
                        // therefore accurately report that the names are
                        // unbound.

                        if value_result.is_unknown() {
                            value_result = UnboundResult;
                        }
                        if type_result.is_unknown() {
                            type_result = UnboundResult;
                        }
                    }
                    Some(import_resolution)
                            if import_resolution.outstanding_references == 0 => {

                        fn get_binding(this: &mut Resolver,
                                       import_resolution: &ImportResolution,
                                       namespace: Namespace)
                                    -> NamespaceResult {

                            // Import resolutions must be declared with "pub"
                            // in order to be exported.
                            if !import_resolution.is_public {
                                return UnboundResult;
                            }

                            match import_resolution.
                                    target_for_namespace(namespace) {
                                None => {
                                    return UnboundResult;
                                }
                                Some(Target {
                                    target_module,
                                    bindings,
                                    shadowable: _
                                }) => {
                                    debug!("(resolving single import) found \
                                            import in ns {}", namespace);
                                    let id = import_resolution.id(namespace);
                                    // track used imports and extern crates as well
                                    this.used_imports.insert((id, namespace));
                                    match target_module.def_id.get() {
                                        Some(DefId{krate: kid, ..}) => {
                                            this.used_crates.insert(kid);
                                        },
                                        _ => {}
                                    }
                                    return BoundResult(target_module, bindings);
                                }
                            }
                        }

                        // The name is an import which has been fully
                        // resolved. We can, therefore, just follow it.
                        if value_result.is_unknown() {
                            value_result = get_binding(self, import_resolution,
                                                       ValueNS);
                            value_used_reexport = import_resolution.is_public;
                        }
                        if type_result.is_unknown() {
                            type_result = get_binding(self, import_resolution,
                                                      TypeNS);
                            type_used_reexport = import_resolution.is_public;
                        }

                    }
                    Some(_) => {
                        // If containing_module is the same module whose import we are resolving
                        // and there it has an unresolved import with the same name as `source`,
                        // then the user is actually trying to import an item that is declared
                        // in the same scope
                        //
                        // e.g
                        // use self::submodule;
                        // pub mod submodule;
                        //
                        // In this case we continue as if we resolved the import and let the
                        // check_for_conflicts_between_imports_and_items call below handle
                        // the conflict
                        match (module_.def_id.get(),  containing_module.def_id.get()) {
                            (Some(id1), Some(id2)) if id1 == id2  => {
                                if value_result.is_unknown() {
                                    value_result = UnboundResult;
                                }
                                if type_result.is_unknown() {
                                    type_result = UnboundResult;
                                }
                            }
                            _ =>  {
                                // The import is unresolved. Bail out.
                                debug!("(resolving single import) unresolved import; \
                                        bailing out");
                                return Indeterminate;
                            }
                        }
                    }
                }
            }
        }

        // If we didn't find a result in the type namespace, search the
        // external modules.
        let mut value_used_public = false;
        let mut type_used_public = false;
        match type_result {
            BoundResult(..) => {}
            _ => {
                match containing_module.external_module_children.borrow_mut()
                                       .get(&source).cloned() {
                    None => {} // Continue.
                    Some(module) => {
                        debug!("(resolving single import) found external \
                                module");
                        // track the module as used.
                        match module.def_id.get() {
                            Some(DefId{krate: kid, ..}) => { self.used_crates.insert(kid); },
                            _ => {}
                        }
                        let name_bindings =
                            Rc::new(Resolver::create_name_bindings_from_module(
                                module));
                        type_result = BoundResult(containing_module.clone(),
                                                  name_bindings);
                        type_used_public = true;
                    }
                }
            }
        }

        // We've successfully resolved the import. Write the results in.
        let mut import_resolutions = module_.import_resolutions.borrow_mut();
        let import_resolution = &mut (*import_resolutions)[target];

        match value_result {
            BoundResult(ref target_module, ref name_bindings) => {
                debug!("(resolving single import) found value target: {}",
                       { name_bindings.value_def.borrow().clone().unwrap().def });
                self.check_for_conflicting_import(
                    &import_resolution.value_target,
                    directive.span,
                    target,
                    ValueNS);

                self.check_that_import_is_importable(
                    &**name_bindings,
                    directive.span,
                    target,
                    ValueNS);

                import_resolution.value_target =
                    Some(Target::new(target_module.clone(),
                                     name_bindings.clone(),
                                     directive.shadowable));
                import_resolution.value_id = directive.id;
                import_resolution.is_public = directive.is_public;
                value_used_public = name_bindings.defined_in_public_namespace(ValueNS);
            }
            UnboundResult => { /* Continue. */ }
            UnknownResult => {
                panic!("value result should be known at this point");
            }
        }
        match type_result {
            BoundResult(ref target_module, ref name_bindings) => {
                debug!("(resolving single import) found type target: {}",
                       { name_bindings.type_def.borrow().clone().unwrap().type_def });
                self.check_for_conflicting_import(
                    &import_resolution.type_target,
                    directive.span,
                    target,
                    TypeNS);

                self.check_that_import_is_importable(
                    &**name_bindings,
                    directive.span,
                    target,
                    TypeNS);

                import_resolution.type_target =
                    Some(Target::new(target_module.clone(),
                                     name_bindings.clone(),
                                     directive.shadowable));
                import_resolution.type_id = directive.id;
                import_resolution.is_public = directive.is_public;
                type_used_public = name_bindings.defined_in_public_namespace(TypeNS);
            }
            UnboundResult => { /* Continue. */ }
            UnknownResult => {
                panic!("type result should be known at this point");
            }
        }

        self.check_for_conflicts_between_imports_and_items(
            module_,
            import_resolution,
            directive.span,
            target);

        if value_result.is_unbound() && type_result.is_unbound() {
            let msg = format!("There is no `{}` in `{}`",
                              token::get_name(source),
                              self.module_to_string(&*containing_module));
            return Failed(Some((directive.span, msg)));
        }
        let value_used_public = value_used_reexport || value_used_public;
        let type_used_public = type_used_reexport || type_used_public;

        assert!(import_resolution.outstanding_references >= 1);
        import_resolution.outstanding_references -= 1;

        // record what this import resolves to for later uses in documentation,
        // this may resolve to either a value or a type, but for documentation
        // purposes it's good enough to just favor one over the other.
        let value_private = match import_resolution.value_target {
            Some(ref target) => {
                let def = target.bindings.def_for_namespace(ValueNS).unwrap();
                self.def_map.borrow_mut().insert(directive.id, def);
                let did = def.def_id();
                if value_used_public {Some(lp)} else {Some(DependsOn(did))}
            },
            // AllPublic here and below is a dummy value, it should never be used because
            // _exists is false.
            None => None,
        };
        let type_private = match import_resolution.type_target {
            Some(ref target) => {
                let def = target.bindings.def_for_namespace(TypeNS).unwrap();
                self.def_map.borrow_mut().insert(directive.id, def);
                let did = def.def_id();
                if type_used_public {Some(lp)} else {Some(DependsOn(did))}
            },
            None => None,
        };

        self.last_private.insert(directive.id, LastImport{value_priv: value_private,
                                                          value_used: Used,
                                                          type_priv: type_private,
                                                          type_used: Used});

        debug!("(resolving single import) successfully resolved import");
        return Success(());
    }

    // Resolves a glob import. Note that this function cannot panic; it either
    // succeeds or bails out (as importing * from an empty module or a module
    // that exports nothing is valid).
    fn resolve_glob_import(&mut self,
                           module_: &Module,
                           containing_module: Rc<Module>,
                           import_directive: &ImportDirective,
                           lp: LastPrivate)
                           -> ResolveResult<()> {
        let id = import_directive.id;
        let is_public = import_directive.is_public;

        // This function works in a highly imperative manner; it eagerly adds
        // everything it can to the list of import resolutions of the module
        // node.
        debug!("(resolving glob import) resolving glob import {}", id);

        // We must bail out if the node has unresolved imports of any kind
        // (including globs).
        if !(*containing_module).all_imports_resolved() {
            debug!("(resolving glob import) target module has unresolved \
                    imports; bailing out");
            return Indeterminate;
        }

        assert_eq!(containing_module.glob_count.get(), 0);

        // Add all resolved imports from the containing module.
        let import_resolutions = containing_module.import_resolutions
                                                  .borrow();
        for (ident, target_import_resolution) in import_resolutions.iter() {
            debug!("(resolving glob import) writing module resolution \
                    {} into `{}`",
                   target_import_resolution.type_target.is_none(),
                   self.module_to_string(module_));

            if !target_import_resolution.is_public {
                debug!("(resolving glob import) nevermind, just kidding");
                continue
            }

            // Here we merge two import resolutions.
            let mut import_resolutions = module_.import_resolutions.borrow_mut();
            match import_resolutions.get_mut(ident) {
                Some(dest_import_resolution) => {
                    // Merge the two import resolutions at a finer-grained
                    // level.

                    match target_import_resolution.value_target {
                        None => {
                            // Continue.
                        }
                        Some(ref value_target) => {
                            dest_import_resolution.value_target =
                                Some(value_target.clone());
                        }
                    }
                    match target_import_resolution.type_target {
                        None => {
                            // Continue.
                        }
                        Some(ref type_target) => {
                            dest_import_resolution.type_target =
                                Some(type_target.clone());
                        }
                    }
                    dest_import_resolution.is_public = is_public;
                    continue;
                }
                None => {}
            }

            // Simple: just copy the old import resolution.
            let mut new_import_resolution = ImportResolution::new(id, is_public);
            new_import_resolution.value_target =
                target_import_resolution.value_target.clone();
            new_import_resolution.type_target =
                target_import_resolution.type_target.clone();

            import_resolutions.insert(*ident, new_import_resolution);
        }

        // Add all children from the containing module.
        self.populate_module_if_necessary(&containing_module);

        for (&name, name_bindings) in containing_module.children
                                                       .borrow().iter() {
            self.merge_import_resolution(module_,
                                         containing_module.clone(),
                                         import_directive,
                                         name,
                                         name_bindings.clone());

        }

        // Add external module children from the containing module.
        for (&name, module) in containing_module.external_module_children
                                                .borrow().iter() {
            let name_bindings =
                Rc::new(Resolver::create_name_bindings_from_module(module.clone()));
            self.merge_import_resolution(module_,
                                         containing_module.clone(),
                                         import_directive,
                                         name,
                                         name_bindings);
        }

        // Record the destination of this import
        match containing_module.def_id.get() {
            Some(did) => {
                self.def_map.borrow_mut().insert(id, DefMod(did));
                self.last_private.insert(id, lp);
            }
            None => {}
        }

        debug!("(resolving glob import) successfully resolved import");
        return Success(());
    }

    fn merge_import_resolution(&mut self,
                               module_: &Module,
                               containing_module: Rc<Module>,
                               import_directive: &ImportDirective,
                               name: Name,
                               name_bindings: Rc<NameBindings>) {
        let id = import_directive.id;
        let is_public = import_directive.is_public;

        let mut import_resolutions = module_.import_resolutions.borrow_mut();
        let dest_import_resolution = match import_resolutions.entry(name) {
            Occupied(entry) => entry.into_mut(),
            Vacant(entry) => {
                // Create a new import resolution from this child.
                entry.set(ImportResolution::new(id, is_public))
            }
        };

        debug!("(resolving glob import) writing resolution `{}` in `{}` \
               to `{}`",
               token::get_name(name).get().to_string(),
               self.module_to_string(&*containing_module),
               self.module_to_string(module_));

        // Merge the child item into the import resolution.
        if name_bindings.defined_in_namespace_with(ValueNS, IMPORTABLE | PUBLIC) {
            debug!("(resolving glob import) ... for value target");
            dest_import_resolution.value_target =
                Some(Target::new(containing_module.clone(),
                                 name_bindings.clone(),
                                 import_directive.shadowable));
            dest_import_resolution.value_id = id;
        }
        if name_bindings.defined_in_namespace_with(TypeNS, IMPORTABLE | PUBLIC) {
            debug!("(resolving glob import) ... for type target");
            dest_import_resolution.type_target =
                Some(Target::new(containing_module,
                                 name_bindings.clone(),
                                 import_directive.shadowable));
            dest_import_resolution.type_id = id;
        }
        dest_import_resolution.is_public = is_public;

        self.check_for_conflicts_between_imports_and_items(
            module_,
            dest_import_resolution,
            import_directive.span,
            name);
    }

    /// Checks that imported names and items don't have the same name.
    fn check_for_conflicting_import(&mut self,
                                    target: &Option<Target>,
                                    import_span: Span,
                                    name: Name,
                                    namespace: Namespace) {
        if self.session.features.borrow().import_shadowing {
            return
        }

        match *target {
            Some(ref target) if !target.shadowable => {
                let msg = format!("a {} named `{}` has already been imported \
                                   in this module",
                                  match namespace {
                                    TypeNS => "type",
                                    ValueNS => "value",
                                  },
                                  token::get_name(name).get());
                self.session.span_err(import_span, msg.as_slice());
            }
            Some(_) | None => {}
        }
    }

    /// Checks that an import is actually importable
    fn check_that_import_is_importable(&mut self,
                                       name_bindings: &NameBindings,
                                       import_span: Span,
                                       name: Name,
                                       namespace: Namespace) {
        if !name_bindings.defined_in_namespace_with(namespace, IMPORTABLE) {
            let msg = format!("`{}` is not directly importable",
                              token::get_name(name));
            self.session.span_err(import_span, msg.as_slice());
        }
    }

    /// Checks that imported names and items don't have the same name.
    fn check_for_conflicts_between_imports_and_items(&mut self,
                                                     module: &Module,
                                                     import_resolution:
                                                     &ImportResolution,
                                                     import_span: Span,
                                                     name: Name) {
        if self.session.features.borrow().import_shadowing {
            return
        }

        // First, check for conflicts between imports and `extern crate`s.
        if module.external_module_children
                 .borrow()
                 .contains_key(&name) {
            match import_resolution.type_target {
                Some(ref target) if !target.shadowable => {
                    let msg = format!("import `{0}` conflicts with imported \
                                       crate in this module \
                                       (maybe you meant `use {0}::*`?)",
                                      token::get_name(name).get());
                    self.session.span_err(import_span, msg.as_slice());
                }
                Some(_) | None => {}
            }
        }

        // Check for item conflicts.
        let children = module.children.borrow();
        let name_bindings = match children.get(&name) {
            None => {
                // There can't be any conflicts.
                return
            }
            Some(ref name_bindings) => (*name_bindings).clone(),
        };

        match import_resolution.value_target {
            Some(ref target) if !target.shadowable => {
                if let Some(ref value) = *name_bindings.value_def.borrow() {
                    let msg = format!("import `{}` conflicts with value \
                                       in this module",
                                      token::get_name(name).get());
                    self.session.span_err(import_span, msg.as_slice());
                    if let Some(span) = value.value_span {
                        self.session.span_note(span,
                                               "conflicting value here");
                    }
                }
            }
            Some(_) | None => {}
        }

        match import_resolution.type_target {
            Some(ref target) if !target.shadowable => {
                if let Some(ref ty) = *name_bindings.type_def.borrow() {
                    match ty.module_def {
                        None => {
                            let msg = format!("import `{}` conflicts with type in \
                                               this module",
                                              token::get_name(name).get());
                            self.session.span_err(import_span, msg.as_slice());
                            if let Some(span) = ty.type_span {
                                self.session.span_note(span,
                                                       "note conflicting type here")
                            }
                        }
                        Some(ref module_def) => {
                            match module_def.kind.get() {
                                ImplModuleKind => {
                                    if let Some(span) = ty.type_span {
                                        let msg = format!("inherent implementations \
                                                           are only allowed on types \
                                                           defined in the current module");
                                        self.session.span_err(span, msg.as_slice());
                                        self.session.span_note(import_span,
                                                               "import from other module here")
                                    }
                                }
                                _ => {
                                    let msg = format!("import `{}` conflicts with existing \
                                                       submodule",
                                                      token::get_name(name).get());
                                    self.session.span_err(import_span, msg.as_slice());
                                    if let Some(span) = ty.type_span {
                                        self.session.span_note(span,
                                                               "note conflicting module here")
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Some(_) | None => {}
        }
    }

    /// Checks that the names of external crates don't collide with other
    /// external crates.
    fn check_for_conflicts_between_external_crates(&self,
                                                   module: &Module,
                                                   name: Name,
                                                   span: Span) {
        if self.session.features.borrow().import_shadowing {
            return
        }

        if module.external_module_children.borrow().contains_key(&name) {
            self.session
                .span_err(span,
                          format!("an external crate named `{}` has already \
                                   been imported into this module",
                                  token::get_name(name).get()).as_slice());
        }
    }

    /// Checks that the names of items don't collide with external crates.
    fn check_for_conflicts_between_external_crates_and_items(&self,
                                                             module: &Module,
                                                             name: Name,
                                                             span: Span) {
        if self.session.features.borrow().import_shadowing {
            return
        }

        if module.external_module_children.borrow().contains_key(&name) {
            self.session
                .span_err(span,
                          format!("the name `{}` conflicts with an external \
                                   crate that has been imported into this \
                                   module",
                                  token::get_name(name).get()).as_slice());
        }
    }

    /// Resolves the given module path from the given root `module_`.
    fn resolve_module_path_from_root(&mut self,
                                     module_: Rc<Module>,
                                     module_path: &[Name],
                                     index: uint,
                                     span: Span,
                                     name_search_type: NameSearchType,
                                     lp: LastPrivate)
                                -> ResolveResult<(Rc<Module>, LastPrivate)> {
        fn search_parent_externals(needle: Name, module: &Rc<Module>)
                                -> Option<Rc<Module>> {
            module.external_module_children.borrow()
                                            .get(&needle).cloned()
                                            .map(|_| module.clone())
                                            .or_else(|| {
                match module.parent_link.clone() {
                    ModuleParentLink(parent, _) => {
                        search_parent_externals(needle,
                                                &parent.upgrade().unwrap())
                    }
                   _ => None
                }
            })
        }

        let mut search_module = module_;
        let mut index = index;
        let module_path_len = module_path.len();
        let mut closest_private = lp;

        // Resolve the module part of the path. This does not involve looking
        // upward though scope chains; we simply resolve names directly in
        // modules as we go.
        while index < module_path_len {
            let name = module_path[index];
            match self.resolve_name_in_module(search_module.clone(),
                                              name,
                                              TypeNS,
                                              name_search_type,
                                              false) {
                Failed(None) => {
                    let segment_name = token::get_name(name);
                    let module_name = self.module_to_string(&*search_module);
                    let mut span = span;
                    let msg = if "???" == module_name.as_slice() {
                        span.hi = span.lo + Pos::from_uint(segment_name.get().len());

                        match search_parent_externals(name,
                                                     &self.current_module) {
                            Some(module) => {
                                let path_str = self.names_to_string(module_path);
                                let target_mod_str = self.module_to_string(&*module);
                                let current_mod_str =
                                    self.module_to_string(&*self.current_module);

                                let prefix = if target_mod_str == current_mod_str {
                                    "self::".to_string()
                                } else {
                                    format!("{}::", target_mod_str)
                                };

                                format!("Did you mean `{}{}`?", prefix, path_str)
                            },
                            None => format!("Maybe a missing `extern crate {}`?",
                                            segment_name),
                        }
                    } else {
                        format!("Could not find `{}` in `{}`",
                                segment_name,
                                module_name)
                    };

                    return Failed(Some((span, msg)));
                }
                Failed(err) => return Failed(err),
                Indeterminate => {
                    debug!("(resolving module path for import) module \
                            resolution is indeterminate: {}",
                            token::get_name(name));
                    return Indeterminate;
                }
                Success((target, used_proxy)) => {
                    // Check to see whether there are type bindings, and, if
                    // so, whether there is a module within.
                    match *target.bindings.type_def.borrow() {
                        Some(ref type_def) => {
                            match type_def.module_def {
                                None => {
                                    let msg = format!("Not a module `{}`",
                                                        token::get_name(name));

                                    return Failed(Some((span, msg)));
                                }
                                Some(ref module_def) => {
                                    search_module = module_def.clone();

                                    // track extern crates for unused_extern_crate lint
                                    if let Some(did) = module_def.def_id.get() {
                                        self.used_crates.insert(did.krate);
                                    }

                                    // Keep track of the closest
                                    // private module used when
                                    // resolving this import chain.
                                    if !used_proxy && !search_module.is_public {
                                        if let Some(did) = search_module.def_id.get() {
                                            closest_private = LastMod(DependsOn(did));
                                        }
                                    }
                                }
                            }
                        }
                        None => {
                            // There are no type bindings at all.
                            let msg = format!("Not a module `{}`",
                                              token::get_name(name));
                            return Failed(Some((span, msg)));
                        }
                    }
                }
            }

            index += 1;
        }

        return Success((search_module, closest_private));
    }

    /// Attempts to resolve the module part of an import directive or path
    /// rooted at the given module.
    ///
    /// On success, returns the resolved module, and the closest *private*
    /// module found to the destination when resolving this path.
    fn resolve_module_path(&mut self,
                           module_: Rc<Module>,
                           module_path: &[Name],
                           use_lexical_scope: UseLexicalScopeFlag,
                           span: Span,
                           name_search_type: NameSearchType)
                               -> ResolveResult<(Rc<Module>, LastPrivate)> {
        let module_path_len = module_path.len();
        assert!(module_path_len > 0);

        debug!("(resolving module path for import) processing `{}` rooted at \
               `{}`",
               self.names_to_string(module_path),
               self.module_to_string(&*module_));

        // Resolve the module prefix, if any.
        let module_prefix_result = self.resolve_module_prefix(module_.clone(),
                                                              module_path);

        let search_module;
        let start_index;
        let last_private;
        match module_prefix_result {
            Failed(None) => {
                let mpath = self.names_to_string(module_path);
                let mpath = mpath.as_slice();
                match mpath.rfind(':') {
                    Some(idx) => {
                        let msg = format!("Could not find `{}` in `{}`",
                                            // idx +- 1 to account for the
                                            // colons on either side
                                            mpath.slice_from(idx + 1),
                                            mpath.slice_to(idx - 1));
                        return Failed(Some((span, msg)));
                    },
                    None => return Failed(None),
                }
            }
            Failed(err) => return Failed(err),
            Indeterminate => {
                debug!("(resolving module path for import) indeterminate; \
                        bailing");
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
                        search_module = self.graph_root.get_module();
                        start_index = 0;
                        last_private = LastMod(AllPublic);
                    }
                    UseLexicalScope => {
                        // This is not a crate-relative path. We resolve the
                        // first component of the path in the current lexical
                        // scope and then proceed to resolve below that.
                        match self.resolve_module_in_lexical_scope(
                                                            module_,
                                                            module_path[0]) {
                            Failed(err) => return Failed(err),
                            Indeterminate => {
                                debug!("(resolving module path for import) \
                                        indeterminate; bailing");
                                return Indeterminate;
                            }
                            Success(containing_module) => {
                                search_module = containing_module;
                                start_index = 1;
                                last_private = LastMod(AllPublic);
                            }
                        }
                    }
                }
            }
            Success(PrefixFound(ref containing_module, index)) => {
                search_module = containing_module.clone();
                start_index = index;
                last_private = LastMod(DependsOn(containing_module.def_id
                                                                  .get()
                                                                  .unwrap()));
            }
        }

        self.resolve_module_path_from_root(search_module,
                                           module_path,
                                           start_index,
                                           span,
                                           name_search_type,
                                           last_private)
    }

    /// Invariant: This must only be called during main resolution, not during
    /// import resolution.
    fn resolve_item_in_lexical_scope(&mut self,
                                     module_: Rc<Module>,
                                     name: Name,
                                     namespace: Namespace)
                                    -> ResolveResult<(Target, bool)> {
        debug!("(resolving item in lexical scope) resolving `{}` in \
                namespace {} in `{}`",
               token::get_name(name),
               namespace,
               self.module_to_string(&*module_));

        // The current module node is handled specially. First, check for
        // its immediate children.
        self.populate_module_if_necessary(&module_);

        match module_.children.borrow().get(&name) {
            Some(name_bindings)
                    if name_bindings.defined_in_namespace(namespace) => {
                debug!("top name bindings succeeded");
                return Success((Target::new(module_.clone(),
                                            name_bindings.clone(),
                                            false),
                               false));
            }
            Some(_) | None => { /* Not found; continue. */ }
        }

        // Now check for its import directives. We don't have to have resolved
        // all its imports in the usual way; this is because chains of
        // adjacent import statements are processed as though they mutated the
        // current scope.
        if let Some(import_resolution) = module_.import_resolutions.borrow().get(&name) {
            match (*import_resolution).target_for_namespace(namespace) {
                None => {
                    // Not found; continue.
                    debug!("(resolving item in lexical scope) found \
                            import resolution, but not in namespace {}",
                           namespace);
                }
                Some(target) => {
                    debug!("(resolving item in lexical scope) using \
                            import resolution");
                    // track used imports and extern crates as well
                    self.used_imports.insert((import_resolution.id(namespace), namespace));
                    if let Some(DefId{krate: kid, ..}) = target.target_module.def_id.get() {
                        self.used_crates.insert(kid);
                    }
                    return Success((target, false));
                }
            }
        }

        // Search for external modules.
        if namespace == TypeNS {
            if let Some(module) = module_.external_module_children.borrow().get(&name).cloned() {
                let name_bindings =
                    Rc::new(Resolver::create_name_bindings_from_module(module));
                debug!("lower name bindings succeeded");
                return Success((Target::new(module_, name_bindings, false),
                                false));
            }
        }

        // Finally, proceed up the scope chain looking for parent modules.
        let mut search_module = module_;
        loop {
            // Go to the next parent.
            match search_module.parent_link.clone() {
                NoParentLink => {
                    // No more parents. This module was unresolved.
                    debug!("(resolving item in lexical scope) unresolved \
                            module");
                    return Failed(None);
                }
                ModuleParentLink(parent_module_node, _) => {
                    match search_module.kind.get() {
                        NormalModuleKind => {
                            // We stop the search here.
                            debug!("(resolving item in lexical \
                                    scope) unresolved module: not \
                                    searching through module \
                                    parents");
                            return Failed(None);
                        }
                        TraitModuleKind |
                        ImplModuleKind |
                        EnumModuleKind |
                        AnonymousModuleKind => {
                            search_module = parent_module_node.upgrade().unwrap();
                        }
                    }
                }
                BlockParentLink(ref parent_module_node, _) => {
                    search_module = parent_module_node.upgrade().unwrap();
                }
            }

            // Resolve the name in the parent module.
            match self.resolve_name_in_module(search_module.clone(),
                                              name,
                                              namespace,
                                              PathSearch,
                                              true) {
                Failed(Some((span, msg))) =>
                    self.resolve_error(span, format!("failed to resolve. {}",
                                                     msg)),
                Failed(None) => (), // Continue up the search chain.
                Indeterminate => {
                    // We couldn't see through the higher scope because of an
                    // unresolved import higher up. Bail.

                    debug!("(resolving item in lexical scope) indeterminate \
                            higher scope; bailing");
                    return Indeterminate;
                }
                Success((target, used_reexport)) => {
                    // We found the module.
                    debug!("(resolving item in lexical scope) found name \
                            in module, done");
                    return Success((target, used_reexport));
                }
            }
        }
    }

    /// Resolves a module name in the current lexical scope.
    fn resolve_module_in_lexical_scope(&mut self,
                                       module_: Rc<Module>,
                                       name: Name)
                                -> ResolveResult<Rc<Module>> {
        // If this module is an anonymous module, resolve the item in the
        // lexical scope. Otherwise, resolve the item from the crate root.
        let resolve_result = self.resolve_item_in_lexical_scope(
            module_, name, TypeNS);
        match resolve_result {
            Success((target, _)) => {
                let bindings = &*target.bindings;
                match *bindings.type_def.borrow() {
                    Some(ref type_def) => {
                        match type_def.module_def {
                            None => {
                                debug!("!!! (resolving module in lexical \
                                        scope) module wasn't actually a \
                                        module!");
                                return Failed(None);
                            }
                            Some(ref module_def) => {
                                return Success(module_def.clone());
                            }
                        }
                    }
                    None => {
                        debug!("!!! (resolving module in lexical scope) module
                                wasn't actually a module!");
                        return Failed(None);
                    }
                }
            }
            Indeterminate => {
                debug!("(resolving module in lexical scope) indeterminate; \
                        bailing");
                return Indeterminate;
            }
            Failed(err) => {
                debug!("(resolving module in lexical scope) failed to resolve");
                return Failed(err);
            }
        }
    }

    /// Returns the nearest normal module parent of the given module.
    fn get_nearest_normal_module_parent(&mut self, module_: Rc<Module>)
                                            -> Option<Rc<Module>> {
        let mut module_ = module_;
        loop {
            match module_.parent_link.clone() {
                NoParentLink => return None,
                ModuleParentLink(new_module, _) |
                BlockParentLink(new_module, _) => {
                    let new_module = new_module.upgrade().unwrap();
                    match new_module.kind.get() {
                        NormalModuleKind => return Some(new_module),
                        TraitModuleKind |
                        ImplModuleKind |
                        EnumModuleKind |
                        AnonymousModuleKind => module_ = new_module,
                    }
                }
            }
        }
    }

    /// Returns the nearest normal module parent of the given module, or the
    /// module itself if it is a normal module.
    fn get_nearest_normal_module_parent_or_self(&mut self, module_: Rc<Module>)
                                                -> Rc<Module> {
        match module_.kind.get() {
            NormalModuleKind => return module_,
            TraitModuleKind |
            ImplModuleKind |
            EnumModuleKind |
            AnonymousModuleKind => {
                match self.get_nearest_normal_module_parent(module_.clone()) {
                    None => module_,
                    Some(new_module) => new_module
                }
            }
        }
    }

    /// Resolves a "module prefix". A module prefix is one or both of (a) `self::`;
    /// (b) some chain of `super::`.
    /// grammar: (SELF MOD_SEP ) ? (SUPER MOD_SEP) *
    fn resolve_module_prefix(&mut self,
                             module_: Rc<Module>,
                             module_path: &[Name])
                                 -> ResolveResult<ModulePrefixResult> {
        // Start at the current module if we see `self` or `super`, or at the
        // top of the crate otherwise.
        let mut containing_module;
        let mut i;
        let first_module_path_string = token::get_name(module_path[0]);
        if "self" == first_module_path_string.get() {
            containing_module =
                self.get_nearest_normal_module_parent_or_self(module_);
            i = 1;
        } else if "super" == first_module_path_string.get() {
            containing_module =
                self.get_nearest_normal_module_parent_or_self(module_);
            i = 0;  // We'll handle `super` below.
        } else {
            return Success(NoPrefixFound);
        }

        // Now loop through all the `super`s we find.
        while i < module_path.len() {
            let string = token::get_name(module_path[i]);
            if "super" != string.get() {
                break
            }
            debug!("(resolving module prefix) resolving `super` at {}",
                   self.module_to_string(&*containing_module));
            match self.get_nearest_normal_module_parent(containing_module) {
                None => return Failed(None),
                Some(new_module) => {
                    containing_module = new_module;
                    i += 1;
                }
            }
        }

        debug!("(resolving module prefix) finished resolving prefix at {}",
               self.module_to_string(&*containing_module));

        return Success(PrefixFound(containing_module, i));
    }

    /// Attempts to resolve the supplied name in the given module for the
    /// given namespace. If successful, returns the target corresponding to
    /// the name.
    ///
    /// The boolean returned on success is an indicator of whether this lookup
    /// passed through a public re-export proxy.
    fn resolve_name_in_module(&mut self,
                              module_: Rc<Module>,
                              name: Name,
                              namespace: Namespace,
                              name_search_type: NameSearchType,
                              allow_private_imports: bool)
                              -> ResolveResult<(Target, bool)> {
        debug!("(resolving name in module) resolving `{}` in `{}`",
               token::get_name(name).get(),
               self.module_to_string(&*module_));

        // First, check the direct children of the module.
        self.populate_module_if_necessary(&module_);

        match module_.children.borrow().get(&name) {
            Some(name_bindings)
                    if name_bindings.defined_in_namespace(namespace) => {
                debug!("(resolving name in module) found node as child");
                return Success((Target::new(module_.clone(),
                                            name_bindings.clone(),
                                            false),
                               false));
            }
            Some(_) | None => {
                // Continue.
            }
        }

        // Next, check the module's imports if necessary.

        // If this is a search of all imports, we should be done with glob
        // resolution at this point.
        if name_search_type == PathSearch {
            assert_eq!(module_.glob_count.get(), 0);
        }

        // Check the list of resolved imports.
        match module_.import_resolutions.borrow().get(&name) {
            Some(import_resolution) if allow_private_imports ||
                                       import_resolution.is_public => {

                if import_resolution.is_public &&
                        import_resolution.outstanding_references != 0 {
                    debug!("(resolving name in module) import \
                           unresolved; bailing out");
                    return Indeterminate;
                }
                match import_resolution.target_for_namespace(namespace) {
                    None => {
                        debug!("(resolving name in module) name found, \
                                but not in namespace {}",
                               namespace);
                    }
                    Some(target) => {
                        debug!("(resolving name in module) resolved to \
                                import");
                        // track used imports and extern crates as well
                        self.used_imports.insert((import_resolution.id(namespace), namespace));
                        if let Some(DefId{krate: kid, ..}) = target.target_module.def_id.get() {
                            self.used_crates.insert(kid);
                        }
                        return Success((target, true));
                    }
                }
            }
            Some(..) | None => {} // Continue.
        }

        // Finally, search through external children.
        if namespace == TypeNS {
            if let Some(module) = module_.external_module_children.borrow().get(&name).cloned() {
                let name_bindings =
                    Rc::new(Resolver::create_name_bindings_from_module(module));
                return Success((Target::new(module_, name_bindings, false),
                                false));
            }
        }

        // We're out of luck.
        debug!("(resolving name in module) failed to resolve `{}`",
               token::get_name(name).get());
        return Failed(None);
    }

    fn report_unresolved_imports(&mut self, module_: Rc<Module>) {
        let index = module_.resolved_import_count.get();
        let imports = module_.imports.borrow();
        let import_count = imports.len();
        if index != import_count {
            let sn = self.session
                         .codemap()
                         .span_to_snippet((*imports)[index].span)
                         .unwrap();
            if sn.contains("::") {
                self.resolve_error((*imports)[index].span,
                                   "unresolved import");
            } else {
                let err = format!("unresolved import (maybe you meant `{}::*`?)",
                                  sn.slice(0, sn.len()));
                self.resolve_error((*imports)[index].span, err.as_slice());
            }
        }

        // Descend into children and anonymous children.
        self.populate_module_if_necessary(&module_);

        for (_, child_node) in module_.children.borrow().iter() {
            match child_node.get_module_if_available() {
                None => {
                    // Continue.
                }
                Some(child_module) => {
                    self.report_unresolved_imports(child_module);
                }
            }
        }

        for (_, module_) in module_.anonymous_children.borrow().iter() {
            self.report_unresolved_imports(module_.clone());
        }
    }

    // Export recording
    //
    // This pass simply determines what all "export" keywords refer to and
    // writes the results into the export map.
    //
    // FIXME #4953 This pass will be removed once exports change to per-item.
    // Then this operation can simply be performed as part of item (or import)
    // processing.

    fn record_exports(&mut self) {
        let root_module = self.graph_root.get_module();
        self.record_exports_for_module_subtree(root_module);
    }

    fn record_exports_for_module_subtree(&mut self,
                                             module_: Rc<Module>) {
        // If this isn't a local krate, then bail out. We don't need to record
        // exports for nonlocal crates.

        match module_.def_id.get() {
            Some(def_id) if def_id.krate == LOCAL_CRATE => {
                // OK. Continue.
                debug!("(recording exports for module subtree) recording \
                        exports for local module `{}`",
                       self.module_to_string(&*module_));
            }
            None => {
                // Record exports for the root module.
                debug!("(recording exports for module subtree) recording \
                        exports for root module `{}`",
                       self.module_to_string(&*module_));
            }
            Some(_) => {
                // Bail out.
                debug!("(recording exports for module subtree) not recording \
                        exports for `{}`",
                       self.module_to_string(&*module_));
                return;
            }
        }

        self.record_exports_for_module(&*module_);
        self.populate_module_if_necessary(&module_);

        for (_, child_name_bindings) in module_.children.borrow().iter() {
            match child_name_bindings.get_module_if_available() {
                None => {
                    // Nothing to do.
                }
                Some(child_module) => {
                    self.record_exports_for_module_subtree(child_module);
                }
            }
        }

        for (_, child_module) in module_.anonymous_children.borrow().iter() {
            self.record_exports_for_module_subtree(child_module.clone());
        }
    }

    fn record_exports_for_module(&mut self, module_: &Module) {
        let mut exports2 = Vec::new();

        self.add_exports_for_module(&mut exports2, module_);
        match module_.def_id.get() {
            Some(def_id) => {
                self.export_map2.insert(def_id.node, exports2);
                debug!("(computing exports) writing exports for {} (some)",
                       def_id.node);
            }
            None => {}
        }
    }

    fn add_exports_of_namebindings(&mut self,
                                   exports2: &mut Vec<Export2> ,
                                   name: Name,
                                   namebindings: &NameBindings,
                                   ns: Namespace) {
        match namebindings.def_for_namespace(ns) {
            Some(d) => {
                let name = token::get_name(name);
                debug!("(computing exports) YES: export '{}' => {}",
                       name, d.def_id());
                exports2.push(Export2 {
                    name: name.get().to_string(),
                    def_id: d.def_id()
                });
            }
            d_opt => {
                debug!("(computing exports) NO: {}", d_opt);
            }
        }
    }

    fn add_exports_for_module(&mut self,
                              exports2: &mut Vec<Export2> ,
                              module_: &Module) {
        for (name, importresolution) in module_.import_resolutions.borrow().iter() {
            if !importresolution.is_public {
                continue
            }
            let xs = [TypeNS, ValueNS];
            for &ns in xs.iter() {
                match importresolution.target_for_namespace(ns) {
                    Some(target) => {
                        debug!("(computing exports) maybe export '{}'",
                               token::get_name(*name));
                        self.add_exports_of_namebindings(exports2,
                                                         *name,
                                                         &*target.bindings,
                                                         ns)
                    }
                    _ => ()
                }
            }
        }
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

    fn with_scope(&mut self, name: Option<Name>, f: |&mut Resolver|) {
        let orig_module = self.current_module.clone();

        // Move down in the graph.
        match name {
            None => {
                // Nothing to do.
            }
            Some(name) => {
                self.populate_module_if_necessary(&orig_module);

                match orig_module.children.borrow().get(&name) {
                    None => {
                        debug!("!!! (with scope) didn't find `{}` in `{}`",
                               token::get_name(name),
                               self.module_to_string(&*orig_module));
                    }
                    Some(name_bindings) => {
                        match (*name_bindings).get_module_if_available() {
                            None => {
                                debug!("!!! (with scope) didn't find module \
                                        for `{}` in `{}`",
                                       token::get_name(name),
                                       self.module_to_string(&*orig_module));
                            }
                            Some(module_) => {
                                self.current_module = module_;
                            }
                        }
                    }
                }
            }
        }

        f(self);

        self.current_module = orig_module;
    }

    /// Wraps the given definition in the appropriate number of `DefUpvar`
    /// wrappers.
    fn upvarify(&self,
                ribs: &[Rib],
                def_like: DefLike,
                span: Span)
                -> Option<DefLike> {
        match def_like {
            DlDef(d @ DefUpvar(..)) => {
                self.session.span_bug(span,
                    format!("unexpected {} in bindings", d).as_slice())
            }
            DlDef(d @ DefLocal(_)) => {
                let node_id = d.def_id().node;
                let mut def = d;
                let mut last_proc_body_id = ast::DUMMY_NODE_ID;
                for rib in ribs.iter() {
                    match rib.kind {
                        NormalRibKind => {
                            // Nothing to do. Continue.
                        }
                        ClosureRibKind(function_id, maybe_proc_body) => {
                            let prev_def = def;
                            if maybe_proc_body != ast::DUMMY_NODE_ID {
                                last_proc_body_id = maybe_proc_body;
                            }
                            def = DefUpvar(node_id, function_id, last_proc_body_id);

                            let mut seen = self.freevars_seen.borrow_mut();
                            let seen = match seen.entry(function_id) {
                                Occupied(v) => v.into_mut(),
                                Vacant(v) => v.set(NodeSet::new()),
                            };
                            if seen.contains(&node_id) {
                                continue;
                            }
                            match self.freevars.borrow_mut().entry(function_id) {
                                Occupied(v) => v.into_mut(),
                                Vacant(v) => v.set(vec![]),
                            }.push(Freevar { def: prev_def, span: span });
                            seen.insert(node_id);
                        }
                        MethodRibKind(item_id, _) => {
                            // If the def is a ty param, and came from the parent
                            // item, it's ok
                            match def {
                                DefTyParam(_, did, _) if {
                                    self.def_map.borrow().get(&did.node).cloned()
                                        == Some(DefTyParamBinder(item_id))
                                } => {} // ok
                                DefSelfTy(did) if did == item_id => {} // ok
                                _ => {
                                    // This was an attempt to access an upvar inside a
                                    // named function item. This is not allowed, so we
                                    // report an error.

                                    self.resolve_error(
                                        span,
                                        "can't capture dynamic environment in a fn item; \
                                        use the || { ... } closure form instead");

                                    return None;
                                }
                            }
                        }
                        ItemRibKind => {
                            // This was an attempt to access an upvar inside a
                            // named function item. This is not allowed, so we
                            // report an error.

                            self.resolve_error(
                                span,
                                "can't capture dynamic environment in a fn item; \
                                use the || { ... } closure form instead");

                            return None;
                        }
                        ConstantItemRibKind => {
                            // Still doesn't deal with upvars
                            self.resolve_error(span,
                                               "attempt to use a non-constant \
                                                value in a constant");

                        }
                    }
                }
                Some(DlDef(def))
            }
            DlDef(def @ DefTyParam(..)) |
            DlDef(def @ DefSelfTy(..)) => {
                for rib in ribs.iter() {
                    match rib.kind {
                        NormalRibKind | ClosureRibKind(..) => {
                            // Nothing to do. Continue.
                        }
                        MethodRibKind(item_id, _) => {
                            // If the def is a ty param, and came from the parent
                            // item, it's ok
                            match def {
                                DefTyParam(_, did, _) if {
                                    self.def_map.borrow().get(&did.node).cloned()
                                        == Some(DefTyParamBinder(item_id))
                                } => {} // ok
                                DefSelfTy(did) if did == item_id => {} // ok

                                _ => {
                                    // This was an attempt to use a type parameter outside
                                    // its scope.

                                    self.resolve_error(span,
                                                        "can't use type parameters from \
                                                        outer function; try using a local \
                                                        type parameter instead");

                                    return None;
                                }
                            }
                        }
                        ItemRibKind => {
                            // This was an attempt to use a type parameter outside
                            // its scope.

                            self.resolve_error(span,
                                               "can't use type parameters from \
                                                outer function; try using a local \
                                                type parameter instead");

                            return None;
                        }
                        ConstantItemRibKind => {
                            // see #9186
                            self.resolve_error(span,
                                               "cannot use an outer type \
                                                parameter in this context");

                        }
                    }
                }
                Some(DlDef(def))
            }
            _ => Some(def_like)
        }
    }

    fn search_ribs(&self,
                   ribs: &[Rib],
                   name: Name,
                   span: Span)
                   -> Option<DefLike> {
        // FIXME #4950: Try caching?

        for (i, rib) in ribs.iter().enumerate().rev() {
            match rib.bindings.get(&name).cloned() {
                Some(def_like) => {
                    return self.upvarify(ribs[i + 1..], def_like, span);
                }
                None => {
                    // Continue.
                }
            }
        }

        None
    }

    fn resolve_crate(&mut self, krate: &ast::Crate) {
        debug!("(resolving crate) starting");

        visit::walk_crate(self, krate);
    }

    fn resolve_item(&mut self, item: &Item) {
        let name = item.ident.name;

        debug!("(resolving item) resolving {}",
               token::get_name(name));

        match item.node {

            // enum item: resolve all the variants' discrs,
            // then resolve the ty params
            ItemEnum(ref enum_def, ref generics) => {
                for variant in (*enum_def).variants.iter() {
                    for dis_expr in variant.node.disr_expr.iter() {
                        // resolve the discriminator expr
                        // as a constant
                        self.with_constant_rib(|this| {
                            this.resolve_expr(&**dis_expr);
                        });
                    }
                }

                // n.b. the discr expr gets visited twice.
                // but maybe it's okay since the first time will signal an
                // error if there is one? -- tjc
                self.with_type_parameter_rib(HasTypeParameters(generics,
                                                               TypeSpace,
                                                               item.id,
                                                               ItemRibKind),
                                             |this| {
                    this.resolve_type_parameters(&generics.ty_params);
                    this.resolve_where_clause(&generics.where_clause);
                    visit::walk_item(this, item);
                });
            }

            ItemTy(_, ref generics) => {
                self.with_type_parameter_rib(HasTypeParameters(generics,
                                                               TypeSpace,
                                                               item.id,
                                                               ItemRibKind),
                                             |this| {
                    this.resolve_type_parameters(&generics.ty_params);
                    visit::walk_item(this, item);
                });
            }

            ItemImpl(ref generics,
                     ref implemented_traits,
                     ref self_type,
                     ref impl_items) => {
                self.resolve_implementation(item.id,
                                            generics,
                                            implemented_traits,
                                            &**self_type,
                                            impl_items.as_slice());
            }

            ItemTrait(ref generics, ref unbound, ref bounds, ref trait_items) => {
                // Create a new rib for the self type.
                let mut self_type_rib = Rib::new(ItemRibKind);

                // plain insert (no renaming, types are not currently hygienic....)
                let name = self.type_self_name;
                self_type_rib.bindings.insert(name, DlDef(DefSelfTy(item.id)));
                self.type_ribs.push(self_type_rib);

                // Create a new rib for the trait-wide type parameters.
                self.with_type_parameter_rib(HasTypeParameters(generics,
                                                               TypeSpace,
                                                               item.id,
                                                               NormalRibKind),
                                             |this| {
                    this.resolve_type_parameters(&generics.ty_params);
                    this.resolve_where_clause(&generics.where_clause);

                    this.resolve_type_parameter_bounds(item.id, bounds,
                                                       TraitDerivation);

                    match *unbound {
                        Some(ref tpb) => {
                            this.resolve_trait_reference(item.id, tpb, TraitDerivation);
                        }
                        None => {}
                    }

                    for trait_item in (*trait_items).iter() {
                        // Create a new rib for the trait_item-specific type
                        // parameters.
                        //
                        // FIXME #4951: Do we need a node ID here?

                        match *trait_item {
                          ast::RequiredMethod(ref ty_m) => {
                            this.with_type_parameter_rib
                                (HasTypeParameters(&ty_m.generics,
                                                   FnSpace,
                                                   item.id,
                                        MethodRibKind(item.id, RequiredMethod)),
                                 |this| {

                                // Resolve the method-specific type
                                // parameters.
                                this.resolve_type_parameters(
                                    &ty_m.generics.ty_params);
                                this.resolve_where_clause(&ty_m.generics
                                                               .where_clause);

                                for argument in ty_m.decl.inputs.iter() {
                                    this.resolve_type(&*argument.ty);
                                }

                                if let SelfExplicit(ref typ, _) = ty_m.explicit_self.node {
                                    this.resolve_type(&**typ)
                                }

                                if let ast::Return(ref ret_ty) = ty_m.decl.output {
                                    this.resolve_type(&**ret_ty);
                                }
                            });
                          }
                          ast::ProvidedMethod(ref m) => {
                              this.resolve_method(MethodRibKind(item.id,
                                                                ProvidedMethod(m.id)),
                                                  &**m)
                          }
                          ast::TypeTraitItem(ref data) => {
                              this.resolve_type_parameter(&data.ty_param);
                              visit::walk_trait_item(this, trait_item);
                          }
                        }
                    }
                });

                self.type_ribs.pop();
            }

            ItemStruct(ref struct_def, ref generics) => {
                self.resolve_struct(item.id,
                                    generics,
                                    struct_def.fields.as_slice());
            }

            ItemMod(ref module_) => {
                self.with_scope(Some(name), |this| {
                    this.resolve_module(module_, item.span, name,
                                        item.id);
                });
            }

            ItemForeignMod(ref foreign_module) => {
                self.with_scope(Some(name), |this| {
                    for foreign_item in foreign_module.items.iter() {
                        match foreign_item.node {
                            ForeignItemFn(_, ref generics) => {
                                this.with_type_parameter_rib(
                                    HasTypeParameters(
                                        generics, FnSpace, foreign_item.id,
                                        ItemRibKind),
                                    |this| visit::walk_foreign_item(this,
                                                                    &**foreign_item));
                            }
                            ForeignItemStatic(..) => {
                                visit::walk_foreign_item(this,
                                                         &**foreign_item);
                            }
                        }
                    }
                });
            }

            ItemFn(ref fn_decl, _, _, ref generics, ref block) => {
                self.resolve_function(ItemRibKind,
                                      Some(&**fn_decl),
                                      HasTypeParameters
                                        (generics,
                                         FnSpace,
                                         item.id,
                                         ItemRibKind),
                                      &**block);
            }

            ItemConst(..) | ItemStatic(..) => {
                self.with_constant_rib(|this| {
                    visit::walk_item(this, item);
                });
            }

           ItemMac(..) => {
                // do nothing, these are just around to be encoded
           }
        }
    }

    fn with_type_parameter_rib(&mut self,
                               type_parameters: TypeParameters,
                               f: |&mut Resolver|) {
        match type_parameters {
            HasTypeParameters(generics, space, node_id, rib_kind) => {
                let mut function_type_rib = Rib::new(rib_kind);
                let mut seen_bindings = HashSet::new();
                for (index, type_parameter) in generics.ty_params.iter().enumerate() {
                    let name = type_parameter.ident.name;
                    debug!("with_type_parameter_rib: {} {}", node_id,
                           type_parameter.id);

                    if seen_bindings.contains(&name) {
                        self.resolve_error(type_parameter.span,
                                           format!("the name `{}` is already \
                                                    used for a type \
                                                    parameter in this type \
                                                    parameter list",
                                                   token::get_name(
                                                       name)).as_slice())
                    }
                    seen_bindings.insert(name);

                    let def_like = DlDef(DefTyParam(space,
                                                    local_def(type_parameter.id),
                                                    index));
                    // Associate this type parameter with
                    // the item that bound it
                    self.record_def(type_parameter.id,
                                    (DefTyParamBinder(node_id), LastMod(AllPublic)));
                    // plain insert (no renaming)
                    function_type_rib.bindings.insert(name, def_like);
                }
                self.type_ribs.push(function_type_rib);
            }

            NoTypeParameters => {
                // Nothing to do.
            }
        }

        f(self);

        match type_parameters {
            HasTypeParameters(..) => { self.type_ribs.pop(); }
            NoTypeParameters => { }
        }
    }

    fn with_label_rib(&mut self, f: |&mut Resolver|) {
        self.label_ribs.push(Rib::new(NormalRibKind));
        f(self);
        self.label_ribs.pop();
    }

    fn with_constant_rib(&mut self, f: |&mut Resolver|) {
        self.value_ribs.push(Rib::new(ConstantItemRibKind));
        self.type_ribs.push(Rib::new(ConstantItemRibKind));
        f(self);
        self.type_ribs.pop();
        self.value_ribs.pop();
    }

    fn resolve_function(&mut self,
                        rib_kind: RibKind,
                        optional_declaration: Option<&FnDecl>,
                        type_parameters: TypeParameters,
                        block: &Block) {
        // Create a value rib for the function.
        let function_value_rib = Rib::new(rib_kind);
        self.value_ribs.push(function_value_rib);

        // Create a label rib for the function.
        let function_label_rib = Rib::new(rib_kind);
        self.label_ribs.push(function_label_rib);

        // If this function has type parameters, add them now.
        self.with_type_parameter_rib(type_parameters, |this| {
            // Resolve the type parameters.
            match type_parameters {
                NoTypeParameters => {
                    // Continue.
                }
                HasTypeParameters(ref generics, _, _, _) => {
                    this.resolve_type_parameters(&generics.ty_params);
                    this.resolve_where_clause(&generics.where_clause);
                }
            }

            // Add each argument to the rib.
            match optional_declaration {
                None => {
                    // Nothing to do.
                }
                Some(declaration) => {
                    let mut bindings_list = HashMap::new();
                    for argument in declaration.inputs.iter() {
                        this.resolve_pattern(&*argument.pat,
                                             ArgumentIrrefutableMode,
                                             &mut bindings_list);

                        this.resolve_type(&*argument.ty);

                        debug!("(resolving function) recorded argument");
                    }

                    if let ast::Return(ref ret_ty) = declaration.output {
                        this.resolve_type(&**ret_ty);
                    }
                }
            }

            // Resolve the function body.
            this.resolve_block(&*block);

            debug!("(resolving function) leaving function");
        });

        self.label_ribs.pop();
        self.value_ribs.pop();
    }

    fn resolve_type_parameters(&mut self,
                               type_parameters: &OwnedSlice<TyParam>) {
        for type_parameter in type_parameters.iter() {
            self.resolve_type_parameter(type_parameter);
        }
    }

    fn resolve_type_parameter(&mut self,
                              type_parameter: &TyParam) {
        for bound in type_parameter.bounds.iter() {
            self.resolve_type_parameter_bound(type_parameter.id, bound,
                                              TraitBoundingTypeParameter);
        }
        match &type_parameter.unbound {
            &Some(ref unbound) =>
                self.resolve_trait_reference(
                    type_parameter.id, unbound, TraitBoundingTypeParameter),
            &None => {}
        }
        match type_parameter.default {
            Some(ref ty) => self.resolve_type(&**ty),
            None => {}
        }
    }

    fn resolve_type_parameter_bounds(&mut self,
                                     id: NodeId,
                                     type_parameter_bounds: &OwnedSlice<TyParamBound>,
                                     reference_type: TraitReferenceType) {
        for type_parameter_bound in type_parameter_bounds.iter() {
            self.resolve_type_parameter_bound(id, type_parameter_bound,
                                              reference_type);
        }
    }

    fn resolve_type_parameter_bound(&mut self,
                                    id: NodeId,
                                    type_parameter_bound: &TyParamBound,
                                    reference_type: TraitReferenceType) {
        match *type_parameter_bound {
            TraitTyParamBound(ref tref) => {
                self.resolve_poly_trait_reference(id, tref, reference_type)
            }
            RegionTyParamBound(..) => {}
        }
    }

    fn resolve_poly_trait_reference(&mut self,
                                    id: NodeId,
                                    poly_trait_reference: &PolyTraitRef,
                                    reference_type: TraitReferenceType) {
        self.resolve_trait_reference(id, &poly_trait_reference.trait_ref, reference_type)
    }

    fn resolve_trait_reference(&mut self,
                               id: NodeId,
                               trait_reference: &TraitRef,
                               reference_type: TraitReferenceType) {
        match self.resolve_path(id, &trait_reference.path, TypeNS, true) {
            None => {
                let path_str = self.path_names_to_string(&trait_reference.path);
                let usage_str = match reference_type {
                    TraitBoundingTypeParameter => "bound type parameter with",
                    TraitImplementation        => "implement",
                    TraitDerivation            => "derive",
                    TraitObject                => "reference",
                    TraitQPath                 => "extract an associated type from",
                };

                let msg = format!("attempt to {} a nonexistent trait `{}`", usage_str, path_str);
                self.resolve_error(trait_reference.path.span, msg.as_slice());
            }
            Some(def) => {
                match def {
                    (DefTrait(_), _) => {
                        debug!("(resolving trait) found trait def: {}", def);
                        self.record_def(trait_reference.ref_id, def);
                    }
                    (def, _) => {
                        self.resolve_error(trait_reference.path.span,
                                           format!("`{}` is not a trait",
                                                   self.path_names_to_string(
                                                       &trait_reference.path)));

                        // If it's a typedef, give a note
                        if let DefTy(..) = def {
                            self.session.span_note(
                                trait_reference.path.span,
                                format!("`type` aliases cannot be used for traits")
                                    .as_slice());
                        }
                    }
                }
            }
        }
    }

    fn resolve_where_clause(&mut self, where_clause: &ast::WhereClause) {
        for predicate in where_clause.predicates.iter() {
            match self.resolve_identifier(predicate.ident,
                                          TypeNS,
                                          true,
                                          predicate.span) {
                Some((def @ DefTyParam(_, _, _), last_private)) => {
                    self.record_def(predicate.id, (def, last_private));
                }
                _ => {
                    self.resolve_error(
                        predicate.span,
                        format!("undeclared type parameter `{}`",
                                token::get_ident(
                                    predicate.ident)).as_slice());
                }
            }

            for bound in predicate.bounds.iter() {
                self.resolve_type_parameter_bound(predicate.id, bound,
                                                  TraitBoundingTypeParameter);
            }
        }
    }

    fn resolve_struct(&mut self,
                      id: NodeId,
                      generics: &Generics,
                      fields: &[StructField]) {
        // If applicable, create a rib for the type parameters.
        self.with_type_parameter_rib(HasTypeParameters(generics,
                                                       TypeSpace,
                                                       id,
                                                       ItemRibKind),
                                     |this| {
            // Resolve the type parameters.
            this.resolve_type_parameters(&generics.ty_params);
            this.resolve_where_clause(&generics.where_clause);

            // Resolve fields.
            for field in fields.iter() {
                this.resolve_type(&*field.node.ty);
            }
        });
    }

    // Does this really need to take a RibKind or is it always going
    // to be NormalRibKind?
    fn resolve_method(&mut self,
                      rib_kind: RibKind,
                      method: &ast::Method) {
        let method_generics = method.pe_generics();
        let type_parameters = HasTypeParameters(method_generics,
                                                FnSpace,
                                                method.id,
                                                rib_kind);

        if let SelfExplicit(ref typ, _) = method.pe_explicit_self().node {
            self.resolve_type(&**typ);
        }

        self.resolve_function(rib_kind,
                              Some(method.pe_fn_decl()),
                              type_parameters,
                              method.pe_body());
    }

    fn with_current_self_type<T>(&mut self, self_type: &Ty, f: |&mut Resolver| -> T) -> T {
        // Handle nested impls (inside fn bodies)
        let previous_value = replace(&mut self.current_self_type, Some(self_type.clone()));
        let result = f(self);
        self.current_self_type = previous_value;
        result
    }

    fn with_optional_trait_ref<T>(&mut self, id: NodeId,
                                  opt_trait_ref: &Option<TraitRef>,
                                  f: |&mut Resolver| -> T) -> T {
        let new_val = match *opt_trait_ref {
            Some(ref trait_ref) => {
                self.resolve_trait_reference(id, trait_ref, TraitImplementation);

                match self.def_map.borrow().get(&trait_ref.ref_id) {
                    Some(def) => {
                        let did = def.def_id();
                        Some((did, trait_ref.clone()))
                    }
                    None => None
                }
            }
            None => None
        };
        let original_trait_ref = replace(&mut self.current_trait_ref, new_val);
        let result = f(self);
        self.current_trait_ref = original_trait_ref;
        result
    }

    fn resolve_implementation(&mut self,
                              id: NodeId,
                              generics: &Generics,
                              opt_trait_reference: &Option<TraitRef>,
                              self_type: &Ty,
                              impl_items: &[ImplItem]) {
        // If applicable, create a rib for the type parameters.
        self.with_type_parameter_rib(HasTypeParameters(generics,
                                                       TypeSpace,
                                                       id,
                                                       NormalRibKind),
                                     |this| {
            // Resolve the type parameters.
            this.resolve_type_parameters(&generics.ty_params);
            this.resolve_where_clause(&generics.where_clause);

            // Resolve the trait reference, if necessary.
            this.with_optional_trait_ref(id, opt_trait_reference, |this| {
                // Resolve the self type.
                this.resolve_type(self_type);

                this.with_current_self_type(self_type, |this| {
                    for impl_item in impl_items.iter() {
                        match *impl_item {
                            MethodImplItem(ref method) => {
                                // If this is a trait impl, ensure the method
                                // exists in trait
                                this.check_trait_item(method.pe_ident().name,
                                                      method.span);

                                // We also need a new scope for the method-
                                // specific type parameters.
                                this.resolve_method(
                                    MethodRibKind(id, ProvidedMethod(method.id)),
                                    &**method);
                            }
                            TypeImplItem(ref typedef) => {
                                // If this is a trait impl, ensure the method
                                // exists in trait
                                this.check_trait_item(typedef.ident.name,
                                                      typedef.span);

                                this.resolve_type(&*typedef.typ);
                            }
                        }
                    }
                });
            });
        });

        // Check that the current type is indeed a type, if we have an anonymous impl
        if opt_trait_reference.is_none() {
            match self_type.node {
                // TyPath is the only thing that we handled in `build_reduced_graph_for_item`,
                // where we created a module with the name of the type in order to implement
                // an anonymous trait. In the case that the path does not resolve to an actual
                // type, the result will be that the type name resolves to a module but not
                // a type (shadowing any imported modules or types with this name), leading
                // to weird user-visible bugs. So we ward this off here. See #15060.
                TyPath(ref path, path_id) => {
                    match self.def_map.borrow().get(&path_id) {
                        // FIXME: should we catch other options and give more precise errors?
                        Some(&DefMod(_)) => {
                            self.resolve_error(path.span, "inherent implementations are not \
                                                           allowed for types not defined in \
                                                           the current module");
                        }
                        _ => {}
                    }
                }
                _ => { }
            }
        }
    }

    fn check_trait_item(&self, name: Name, span: Span) {
        // If there is a TraitRef in scope for an impl, then the method must be in the trait.
        for &(did, ref trait_ref) in self.current_trait_ref.iter() {
            if self.trait_item_map.get(&(name, did)).is_none() {
                let path_str = self.path_names_to_string(&trait_ref.path);
                self.resolve_error(span,
                                    format!("method `{}` is not a member of trait `{}`",
                                            token::get_name(name),
                                            path_str).as_slice());
            }
        }
    }

    fn resolve_module(&mut self, module: &Mod, _span: Span,
                      _name: Name, id: NodeId) {
        // Write the implementations in scope into the module metadata.
        debug!("(resolving module) resolving module ID {}", id);
        visit::walk_mod(self, module);
    }

    fn resolve_local(&mut self, local: &Local) {
        // Resolve the type.
        self.resolve_type(&*local.ty);

        // Resolve the initializer, if necessary.
        match local.init {
            None => {
                // Nothing to do.
            }
            Some(ref initializer) => {
                self.resolve_expr(&**initializer);
            }
        }

        // Resolve the pattern.
        let mut bindings_list = HashMap::new();
        self.resolve_pattern(&*local.pat,
                             LocalIrrefutableMode,
                             &mut bindings_list);
    }

    // build a map from pattern identifiers to binding-info's.
    // this is done hygienically. This could arise for a macro
    // that expands into an or-pattern where one 'x' was from the
    // user and one 'x' came from the macro.
    fn binding_mode_map(&mut self, pat: &Pat) -> BindingMap {
        let mut result = HashMap::new();
        pat_bindings(&self.def_map, pat, |binding_mode, _id, sp, path1| {
            let name = mtwt::resolve(path1.node);
            result.insert(name,
                          binding_info {span: sp,
                                        binding_mode: binding_mode});
        });
        return result;
    }

    // check that all of the arms in an or-pattern have exactly the
    // same set of bindings, with the same binding modes for each.
    fn check_consistent_bindings(&mut self, arm: &Arm) {
        if arm.pats.len() == 0 {
            return
        }
        let map_0 = self.binding_mode_map(&*arm.pats[0]);
        for (i, p) in arm.pats.iter().enumerate() {
            let map_i = self.binding_mode_map(&**p);

            for (&key, &binding_0) in map_0.iter() {
                match map_i.get(&key) {
                  None => {
                    self.resolve_error(
                        p.span,
                        format!("variable `{}` from pattern #1 is \
                                  not bound in pattern #{}",
                                token::get_name(key),
                                i + 1).as_slice());
                  }
                  Some(binding_i) => {
                    if binding_0.binding_mode != binding_i.binding_mode {
                        self.resolve_error(
                            binding_i.span,
                            format!("variable `{}` is bound with different \
                                      mode in pattern #{} than in pattern #1",
                                    token::get_name(key),
                                    i + 1).as_slice());
                    }
                  }
                }
            }

            for (&key, &binding) in map_i.iter() {
                if !map_0.contains_key(&key) {
                    self.resolve_error(
                        binding.span,
                        format!("variable `{}` from pattern {}{} is \
                                  not bound in pattern {}1",
                                token::get_name(key),
                                "#", i + 1, "#").as_slice());
                }
            }
        }
    }

    fn resolve_arm(&mut self, arm: &Arm) {
        self.value_ribs.push(Rib::new(NormalRibKind));

        let mut bindings_list = HashMap::new();
        for pattern in arm.pats.iter() {
            self.resolve_pattern(&**pattern, RefutableMode, &mut bindings_list);
        }

        // This has to happen *after* we determine which
        // pat_idents are variants
        self.check_consistent_bindings(arm);

        visit::walk_expr_opt(self, &arm.guard);
        self.resolve_expr(&*arm.body);

        self.value_ribs.pop();
    }

    fn resolve_block(&mut self, block: &Block) {
        debug!("(resolving block) entering block");
        self.value_ribs.push(Rib::new(NormalRibKind));

        // Move down in the graph, if there's an anonymous module rooted here.
        let orig_module = self.current_module.clone();
        match orig_module.anonymous_children.borrow().get(&block.id) {
            None => { /* Nothing to do. */ }
            Some(anonymous_module) => {
                debug!("(resolving block) found anonymous module, moving \
                        down");
                self.current_module = anonymous_module.clone();
            }
        }

        // Descend into the block.
        visit::walk_block(self, block);

        // Move back up.
        self.current_module = orig_module;

        self.value_ribs.pop();
        debug!("(resolving block) leaving block");
    }

    fn resolve_type(&mut self, ty: &Ty) {
        match ty.node {
            // Like path expressions, the interpretation of path types depends
            // on whether the path has multiple elements in it or not.

            TyPath(ref path, path_id) => {
                // This is a path in the type namespace. Walk through scopes
                // looking for it.
                let mut result_def = None;

                // First, check to see whether the name is a primitive type.
                if path.segments.len() == 1 {
                    let id = path.segments.last().unwrap().identifier;

                    match self.primitive_type_table
                            .primitive_types
                            .get(&id.name) {

                        Some(&primitive_type) => {
                            result_def =
                                Some((DefPrimTy(primitive_type), LastMod(AllPublic)));

                            if path.segments
                                   .iter()
                                   .any(|s| s.parameters.has_lifetimes()) {
                                span_err!(self.session, path.span, E0157,
                                    "lifetime parameters are not allowed on this type");
                            } else if path.segments
                                          .iter()
                                          .any(|s| !s.parameters.is_empty()) {
                                span_err!(self.session, path.span, E0153,
                                    "type parameters are not allowed on this type");
                            }
                        }
                        None => {
                            // Continue.
                        }
                    }
                }

                match result_def {
                    None => {
                        match self.resolve_path(ty.id, path, TypeNS, true) {
                            Some(def) => {
                                debug!("(resolving type) resolved `{}` to \
                                        type {}",
                                       token::get_ident(path.segments
                                                            .last().unwrap()
                                                            .identifier),
                                       def);
                                result_def = Some(def);
                            }
                            None => {
                                result_def = None;
                            }
                        }
                    }
                    Some(_) => {}   // Continue.
                }

                match result_def {
                    Some(def) => {
                        // Write the result into the def map.
                        debug!("(resolving type) writing resolution for `{}` \
                                (id {})",
                               self.path_names_to_string(path),
                               path_id);
                        self.record_def(path_id, def);
                    }
                    None => {
                        let msg = format!("use of undeclared type name `{}`",
                                          self.path_names_to_string(path));
                        self.resolve_error(ty.span, msg.as_slice());
                    }
                }
            }

            TyObjectSum(ref ty, ref bound_vec) => {
                self.resolve_type(&**ty);
                self.resolve_type_parameter_bounds(ty.id, bound_vec,
                                                       TraitBoundingTypeParameter);
            }

            TyQPath(ref qpath) => {
                self.resolve_type(&*qpath.self_type);
                self.resolve_trait_reference(ty.id, &*qpath.trait_ref, TraitQPath);
            }

            TyClosure(ref c) | TyProc(ref c) => {
                self.resolve_type_parameter_bounds(
                    ty.id,
                    &c.bounds,
                    TraitBoundingTypeParameter);
                visit::walk_ty(self, ty);
            }

            TyPolyTraitRef(ref bounds) => {
                self.resolve_type_parameter_bounds(
                    ty.id,
                    bounds,
                    TraitObject);
                visit::walk_ty(self, ty);
            }
            _ => {
                // Just resolve embedded types.
                visit::walk_ty(self, ty);
            }
        }
    }

    fn resolve_pattern(&mut self,
                       pattern: &Pat,
                       mode: PatternBindingMode,
                       // Maps idents to the node ID for the (outermost)
                       // pattern that binds them
                       bindings_list: &mut HashMap<Name, NodeId>) {
        let pat_id = pattern.id;
        walk_pat(pattern, |pattern| {
            match pattern.node {
                PatIdent(binding_mode, ref path1, _) => {

                    // The meaning of pat_ident with no type parameters
                    // depends on whether an enum variant or unit-like struct
                    // with that name is in scope. The probing lookup has to
                    // be careful not to emit spurious errors. Only matching
                    // patterns (match) can match nullary variants or
                    // unit-like structs. For binding patterns (let), matching
                    // such a value is simply disallowed (since it's rarely
                    // what you want).

                    let ident = path1.node;
                    let renamed = mtwt::resolve(ident);

                    match self.resolve_bare_identifier_pattern(ident.name, pattern.span) {
                        FoundStructOrEnumVariant(ref def, lp)
                                if mode == RefutableMode => {
                            debug!("(resolving pattern) resolving `{}` to \
                                    struct or enum variant",
                                   token::get_name(renamed));

                            self.enforce_default_binding_mode(
                                pattern,
                                binding_mode,
                                "an enum variant");
                            self.record_def(pattern.id, (def.clone(), lp));
                        }
                        FoundStructOrEnumVariant(..) => {
                            self.resolve_error(
                                pattern.span,
                                format!("declaration of `{}` shadows an enum \
                                         variant or unit-like struct in \
                                         scope",
                                        token::get_name(renamed)).as_slice());
                        }
                        FoundConst(ref def, lp) if mode == RefutableMode => {
                            debug!("(resolving pattern) resolving `{}` to \
                                    constant",
                                   token::get_name(renamed));

                            self.enforce_default_binding_mode(
                                pattern,
                                binding_mode,
                                "a constant");
                            self.record_def(pattern.id, (def.clone(), lp));
                        }
                        FoundConst(..) => {
                            self.resolve_error(pattern.span,
                                                  "only irrefutable patterns \
                                                   allowed here");
                        }
                        BareIdentifierPatternUnresolved => {
                            debug!("(resolving pattern) binding `{}`",
                                   token::get_name(renamed));

                            let def = DefLocal(pattern.id);

                            // Record the definition so that later passes
                            // will be able to distinguish variants from
                            // locals in patterns.

                            self.record_def(pattern.id, (def, LastMod(AllPublic)));

                            // Add the binding to the local ribs, if it
                            // doesn't already exist in the bindings list. (We
                            // must not add it if it's in the bindings list
                            // because that breaks the assumptions later
                            // passes make about or-patterns.)
                            if !bindings_list.contains_key(&renamed) {
                                let this = &mut *self;
                                let last_rib = this.value_ribs.last_mut().unwrap();
                                last_rib.bindings.insert(renamed, DlDef(def));
                                bindings_list.insert(renamed, pat_id);
                            } else if mode == ArgumentIrrefutableMode &&
                                    bindings_list.contains_key(&renamed) {
                                // Forbid duplicate bindings in the same
                                // parameter list.
                                self.resolve_error(pattern.span,
                                                   format!("identifier `{}` \
                                                            is bound more \
                                                            than once in \
                                                            this parameter \
                                                            list",
                                                           token::get_ident(
                                                               ident))
                                                   .as_slice())
                            } else if bindings_list.get(&renamed) ==
                                    Some(&pat_id) {
                                // Then this is a duplicate variable in the
                                // same disjunction, which is an error.
                                self.resolve_error(pattern.span,
                                    format!("identifier `{}` is bound \
                                             more than once in the same \
                                             pattern",
                                            token::get_ident(ident)).as_slice());
                            }
                            // Else, not bound in the same pattern: do
                            // nothing.
                        }
                    }
                }

                PatEnum(ref path, _) => {
                    // This must be an enum variant, struct or const.
                    match self.resolve_path(pat_id, path, ValueNS, false) {
                        Some(def @ (DefVariant(..), _)) |
                        Some(def @ (DefStruct(..), _))  |
                        Some(def @ (DefConst(..), _)) => {
                            self.record_def(pattern.id, def);
                        }
                        Some((DefStatic(..), _)) => {
                            self.resolve_error(path.span,
                                               "static variables cannot be \
                                                referenced in a pattern, \
                                                use a `const` instead");
                        }
                        Some(_) => {
                            self.resolve_error(path.span,
                                format!("`{}` is not an enum variant, struct or const",
                                    token::get_ident(
                                        path.segments
                                            .last()
                                            .unwrap()
                                            .identifier)).as_slice());
                        }
                        None => {
                            self.resolve_error(path.span,
                                format!("unresolved enum variant, struct or const `{}`",
                                    token::get_ident(
                                        path.segments
                                            .last()
                                            .unwrap()
                                            .identifier)).as_slice());
                        }
                    }

                    // Check the types in the path pattern.
                    for ty in path.segments
                                  .iter()
                                  .flat_map(|s| s.parameters.types().into_iter()) {
                        self.resolve_type(&**ty);
                    }
                }

                PatLit(ref expr) => {
                    self.resolve_expr(&**expr);
                }

                PatRange(ref first_expr, ref last_expr) => {
                    self.resolve_expr(&**first_expr);
                    self.resolve_expr(&**last_expr);
                }

                PatStruct(ref path, _, _) => {
                    match self.resolve_path(pat_id, path, TypeNS, false) {
                        Some(definition) => {
                            self.record_def(pattern.id, definition);
                        }
                        result => {
                            debug!("(resolving pattern) didn't find struct \
                                    def: {}", result);
                            let msg = format!("`{}` does not name a structure",
                                              self.path_names_to_string(path));
                            self.resolve_error(path.span, msg.as_slice());
                        }
                    }
                }

                _ => {
                    // Nothing to do.
                }
            }
            true
        });
    }

    fn resolve_bare_identifier_pattern(&mut self, name: Name, span: Span)
                                       -> BareIdentifierPatternResolution {
        let module = self.current_module.clone();
        match self.resolve_item_in_lexical_scope(module,
                                                 name,
                                                 ValueNS) {
            Success((target, _)) => {
                debug!("(resolve bare identifier pattern) succeeded in \
                         finding {} at {}",
                        token::get_name(name),
                        target.bindings.value_def.borrow());
                match *target.bindings.value_def.borrow() {
                    None => {
                        panic!("resolved name in the value namespace to a \
                              set of name bindings with no def?!");
                    }
                    Some(def) => {
                        // For the two success cases, this lookup can be
                        // considered as not having a private component because
                        // the lookup happened only within the current module.
                        match def.def {
                            def @ DefVariant(..) | def @ DefStruct(..) => {
                                return FoundStructOrEnumVariant(def, LastMod(AllPublic));
                            }
                            def @ DefConst(..) => {
                                return FoundConst(def, LastMod(AllPublic));
                            }
                            DefStatic(..) => {
                                self.resolve_error(span,
                                                   "static variables cannot be \
                                                    referenced in a pattern, \
                                                    use a `const` instead");
                                return BareIdentifierPatternUnresolved;
                            }
                            _ => {
                                return BareIdentifierPatternUnresolved;
                            }
                        }
                    }
                }
            }

            Indeterminate => {
                panic!("unexpected indeterminate result");
            }
            Failed(err) => {
                match err {
                    Some((span, msg)) => {
                        self.resolve_error(span, format!("failed to resolve: {}",
                                                         msg));
                    }
                    None => ()
                }

                debug!("(resolve bare identifier pattern) failed to find {}",
                        token::get_name(name));
                return BareIdentifierPatternUnresolved;
            }
        }
    }

    /// If `check_ribs` is true, checks the local definitions first; i.e.
    /// doesn't skip straight to the containing module.
    fn resolve_path(&mut self,
                    id: NodeId,
                    path: &Path,
                    namespace: Namespace,
                    check_ribs: bool) -> Option<(Def, LastPrivate)> {
        // First, resolve the types.
        for ty in path.segments.iter().flat_map(|s| s.parameters.types().into_iter()) {
            self.resolve_type(&**ty);
        }

        if path.global {
            return self.resolve_crate_relative_path(path, namespace);
        }

        let unqualified_def =
                self.resolve_identifier(path.segments
                                            .last().unwrap()
                                            .identifier,
                                        namespace,
                                        check_ribs,
                                        path.span);

        if path.segments.len() > 1 {
            let def = self.resolve_module_relative_path(path, namespace);
            match (def, unqualified_def) {
                (Some((ref d, _)), Some((ref ud, _))) if *d == *ud => {
                    self.session
                        .add_lint(lint::builtin::UNUSED_QUALIFICATIONS,
                                  id,
                                  path.span,
                                  "unnecessary qualification".to_string());
                }
                _ => ()
            }

            return def;
        }

        return unqualified_def;
    }

    // resolve a single identifier (used as a varref)
    fn resolve_identifier(&mut self,
                              identifier: Ident,
                              namespace: Namespace,
                              check_ribs: bool,
                              span: Span)
                              -> Option<(Def, LastPrivate)> {
        if check_ribs {
            match self.resolve_identifier_in_local_ribs(identifier,
                                                      namespace,
                                                      span) {
                Some(def) => {
                    return Some((def, LastMod(AllPublic)));
                }
                None => {
                    // Continue.
                }
            }
        }

        return self.resolve_item_by_name_in_lexical_scope(identifier.name, namespace);
    }

    // FIXME #4952: Merge me with resolve_name_in_module?
    fn resolve_definition_of_name_in_module(&mut self,
                                            containing_module: Rc<Module>,
                                            name: Name,
                                            namespace: Namespace)
                                                -> NameDefinition {
        // First, search children.
        self.populate_module_if_necessary(&containing_module);

        match containing_module.children.borrow().get(&name) {
            Some(child_name_bindings) => {
                match child_name_bindings.def_for_namespace(namespace) {
                    Some(def) => {
                        // Found it. Stop the search here.
                        let p = child_name_bindings.defined_in_public_namespace(
                                        namespace);
                        let lp = if p {LastMod(AllPublic)} else {
                            LastMod(DependsOn(def.def_id()))
                        };
                        return ChildNameDefinition(def, lp);
                    }
                    None => {}
                }
            }
            None => {}
        }

        // Next, search import resolutions.
        match containing_module.import_resolutions.borrow().get(&name) {
            Some(import_resolution) if import_resolution.is_public => {
                if let Some(target) = (*import_resolution).target_for_namespace(namespace) {
                    match target.bindings.def_for_namespace(namespace) {
                        Some(def) => {
                            // Found it.
                            let id = import_resolution.id(namespace);
                            // track imports and extern crates as well
                            self.used_imports.insert((id, namespace));
                            match target.target_module.def_id.get() {
                                Some(DefId{krate: kid, ..}) => {
                                    self.used_crates.insert(kid);
                                },
                                _ => {}
                            }
                            return ImportNameDefinition(def, LastMod(AllPublic));
                        }
                        None => {
                            // This can happen with external impls, due to
                            // the imperfect way we read the metadata.
                        }
                    }
                }
            }
            Some(..) | None => {} // Continue.
        }

        // Finally, search through external children.
        if namespace == TypeNS {
            if let Some(module) = containing_module.external_module_children.borrow()
                                                   .get(&name).cloned() {
                if let Some(def_id) = module.def_id.get() {
                    // track used crates
                    self.used_crates.insert(def_id.krate);
                    let lp = if module.is_public {LastMod(AllPublic)} else {
                        LastMod(DependsOn(def_id))
                    };
                    return ChildNameDefinition(DefMod(def_id), lp);
                }
            }
        }

        return NoNameDefinition;
    }

    // resolve a "module-relative" path, e.g. a::b::c
    fn resolve_module_relative_path(&mut self,
                                        path: &Path,
                                        namespace: Namespace)
                                        -> Option<(Def, LastPrivate)> {
        let module_path = path.segments.init().iter()
                                              .map(|ps| ps.identifier.name)
                                              .collect::<Vec<_>>();

        let containing_module;
        let last_private;
        let module = self.current_module.clone();
        match self.resolve_module_path(module,
                                       module_path.as_slice(),
                                       UseLexicalScope,
                                       path.span,
                                       PathSearch) {
            Failed(err) => {
                let (span, msg) = match err {
                    Some((span, msg)) => (span, msg),
                    None => {
                        let msg = format!("Use of undeclared module `{}`",
                                          self.names_to_string(
                                               module_path.as_slice()));
                        (path.span, msg)
                    }
                };

                self.resolve_error(span, format!("failed to resolve. {}",
                                                 msg.as_slice()));
                return None;
            }
            Indeterminate => panic!("indeterminate unexpected"),
            Success((resulting_module, resulting_last_private)) => {
                containing_module = resulting_module;
                last_private = resulting_last_private;
            }
        }

        let name = path.segments.last().unwrap().identifier.name;
        let def = match self.resolve_definition_of_name_in_module(containing_module.clone(),
                                                                  name,
                                                                  namespace) {
            NoNameDefinition => {
                // We failed to resolve the name. Report an error.
                return None;
            }
            ChildNameDefinition(def, lp) | ImportNameDefinition(def, lp) => {
                (def, last_private.or(lp))
            }
        };
        if let Some(DefId{krate: kid, ..}) = containing_module.def_id.get() {
            self.used_crates.insert(kid);
        }
        return Some(def);
    }

    /// Invariant: This must be called only during main resolution, not during
    /// import resolution.
    fn resolve_crate_relative_path(&mut self,
                                   path: &Path,
                                   namespace: Namespace)
                                       -> Option<(Def, LastPrivate)> {
        let module_path = path.segments.init().iter()
                                              .map(|ps| ps.identifier.name)
                                              .collect::<Vec<_>>();

        let root_module = self.graph_root.get_module();

        let containing_module;
        let last_private;
        match self.resolve_module_path_from_root(root_module,
                                                 module_path.as_slice(),
                                                 0,
                                                 path.span,
                                                 PathSearch,
                                                 LastMod(AllPublic)) {
            Failed(err) => {
                let (span, msg) = match err {
                    Some((span, msg)) => (span, msg),
                    None => {
                        let msg = format!("Use of undeclared module `::{}`",
                                          self.names_to_string(module_path.as_slice()));
                        (path.span, msg)
                    }
                };

                self.resolve_error(span, format!("failed to resolve. {}",
                                                 msg.as_slice()));
                return None;
            }

            Indeterminate => {
                panic!("indeterminate unexpected");
            }

            Success((resulting_module, resulting_last_private)) => {
                containing_module = resulting_module;
                last_private = resulting_last_private;
            }
        }

        let name = path.segments.last().unwrap().identifier.name;
        match self.resolve_definition_of_name_in_module(containing_module,
                                                        name,
                                                        namespace) {
            NoNameDefinition => {
                // We failed to resolve the name. Report an error.
                return None;
            }
            ChildNameDefinition(def, lp) | ImportNameDefinition(def, lp) => {
                return Some((def, last_private.or(lp)));
            }
        }
    }

    fn resolve_identifier_in_local_ribs(&mut self,
                                            ident: Ident,
                                            namespace: Namespace,
                                            span: Span)
                                            -> Option<Def> {
        // Check the local set of ribs.
        let search_result = match namespace {
            ValueNS => {
                let renamed = mtwt::resolve(ident);
                self.search_ribs(self.value_ribs.as_slice(),
                                 renamed, span)
            }
            TypeNS => {
                let name = ident.name;
                self.search_ribs(self.type_ribs.as_slice(), name, span)
            }
        };

        match search_result {
            Some(DlDef(def)) => {
                debug!("(resolving path in local ribs) resolved `{}` to \
                        local: {}",
                       token::get_ident(ident),
                       def);
                return Some(def);
            }
            Some(DlField) | Some(DlImpl(_)) | None => {
                return None;
            }
        }
    }

    fn resolve_item_by_name_in_lexical_scope(&mut self,
                                             name: Name,
                                             namespace: Namespace)
                                            -> Option<(Def, LastPrivate)> {
        // Check the items.
        let module = self.current_module.clone();
        match self.resolve_item_in_lexical_scope(module,
                                                 name,
                                                 namespace) {
            Success((target, _)) => {
                match (*target.bindings).def_for_namespace(namespace) {
                    None => {
                        // This can happen if we were looking for a type and
                        // found a module instead. Modules don't have defs.
                        debug!("(resolving item path by identifier in lexical \
                                 scope) failed to resolve {} after success...",
                                 token::get_name(name));
                        return None;
                    }
                    Some(def) => {
                        debug!("(resolving item path in lexical scope) \
                                resolved `{}` to item",
                               token::get_name(name));
                        // This lookup is "all public" because it only searched
                        // for one identifier in the current module (couldn't
                        // have passed through reexports or anything like that.
                        return Some((def, LastMod(AllPublic)));
                    }
                }
            }
            Indeterminate => {
                panic!("unexpected indeterminate result");
            }
            Failed(err) => {
                match err {
                    Some((span, msg)) =>
                        self.resolve_error(span, format!("failed to resolve. {}", msg)),
                    None => ()
                }

                debug!("(resolving item path by identifier in lexical scope) \
                         failed to resolve {}", token::get_name(name));
                return None;
            }
        }
    }

    fn with_no_errors<T>(&mut self, f: |&mut Resolver| -> T) -> T {
        self.emit_errors = false;
        let rs = f(self);
        self.emit_errors = true;
        rs
    }

    fn resolve_error<T: Str>(&self, span: Span, s: T) {
        if self.emit_errors {
            self.session.span_err(span, s.as_slice());
        }
    }

    fn find_fallback_in_self_type(&mut self, name: Name) -> FallbackSuggestion {
        fn extract_path_and_node_id(t: &Ty, allow: FallbackChecks)
                                                    -> Option<(Path, NodeId, FallbackChecks)> {
            match t.node {
                TyPath(ref path, node_id) => Some((path.clone(), node_id, allow)),
                TyPtr(ref mut_ty) => extract_path_and_node_id(&*mut_ty.ty, OnlyTraitAndStatics),
                TyRptr(_, ref mut_ty) => extract_path_and_node_id(&*mut_ty.ty, allow),
                // This doesn't handle the remaining `Ty` variants as they are not
                // that commonly the self_type, it might be interesting to provide
                // support for those in future.
                _ => None,
            }
        }

        fn get_module(this: &mut Resolver, span: Span, name_path: &[ast::Name])
                            -> Option<Rc<Module>> {
            let root = this.current_module.clone();
            let last_name = name_path.last().unwrap();

            if name_path.len() == 1 {
                match this.primitive_type_table.primitive_types.get(last_name) {
                    Some(_) => None,
                    None => {
                        match this.current_module.children.borrow().get(last_name) {
                            Some(child) => child.get_module_if_available(),
                            None => None
                        }
                    }
                }
            } else {
                match this.resolve_module_path(root,
                                                name_path.as_slice(),
                                                UseLexicalScope,
                                                span,
                                                PathSearch) {
                    Success((module, _)) => Some(module),
                    _ => None
                }
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
            match self.def_map.borrow().get(&node_id) {
                 Some(&DefTy(did, _))
                | Some(&DefStruct(did))
                | Some(&DefVariant(_, did, _)) => match self.structs.get(&did) {
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
        match get_module(self, path.span, name_path.as_slice()) {
            Some(module) => match module.children.borrow().get(&name) {
                Some(binding) => {
                    let p_str = self.path_names_to_string(&path);
                    match binding.def_for_namespace(ValueNS) {
                        Some(DefStaticMethod(_, provenance)) => {
                            match provenance {
                                FromImpl(_) => return StaticMethod(p_str),
                                FromTrait(_) => unreachable!()
                            }
                        }
                        Some(DefMethod(_, None, _)) if allowed == Everything => return Method,
                        Some(DefMethod(_, Some(_), _)) => return TraitItem,
                        _ => ()
                    }
                }
                None => {}
            },
            None => {}
        }

        // Look for a method in the current trait.
        match self.current_trait_ref {
            Some((did, ref trait_ref)) => {
                let path_str = self.path_names_to_string(&trait_ref.path);

                match self.trait_item_map.get(&(name, did)) {
                    Some(&StaticMethodTraitItemKind) => {
                        return TraitMethod(path_str)
                    }
                    Some(_) => return TraitItem,
                    None => {}
                }
            }
            None => {}
        }

        NoSuggestion
    }

    fn find_best_match_for_name(&mut self, name: &str, max_distance: uint)
                                -> Option<String> {
        let this = &mut *self;

        let mut maybes: Vec<token::InternedString> = Vec::new();
        let mut values: Vec<uint> = Vec::new();

        for rib in this.value_ribs.iter().rev() {
            for (&k, _) in rib.bindings.iter() {
                maybes.push(token::get_name(k));
                values.push(uint::MAX);
            }
        }

        let mut smallest = 0;
        for (i, other) in maybes.iter().enumerate() {
            values[i] = name.lev_distance(other.get());

            if values[i] <= values[smallest] {
                smallest = i;
            }
        }

        if values.len() > 0 &&
            values[smallest] != uint::MAX &&
            values[smallest] < name.len() + 2 &&
            values[smallest] <= max_distance &&
            name != maybes[smallest].get() {

            Some(maybes[smallest].get().to_string())

        } else {
            None
        }
    }

    fn resolve_expr(&mut self, expr: &Expr) {
        // First, record candidate traits for this expression if it could
        // result in the invocation of a method call.

        self.record_candidate_traits_for_expr_if_necessary(expr);

        // Next, resolve the node.
        match expr.node {
            // The interpretation of paths depends on whether the path has
            // multiple elements in it or not.

            ExprPath(ref path) => {
                // This is a local path in the value namespace. Walk through
                // scopes looking for it.

                match self.resolve_path(expr.id, path, ValueNS, true) {
                    Some(def) => {
                        // Write the result into the def map.
                        debug!("(resolving expr) resolved `{}`",
                               self.path_names_to_string(path));

                        self.record_def(expr.id, def);
                    }
                    None => {
                        let wrong_name = self.path_names_to_string(path);
                        // Be helpful if the name refers to a struct
                        // (The pattern matching def_tys where the id is in self.structs
                        // matches on regular structs while excluding tuple- and enum-like
                        // structs, which wouldn't result in this error.)
                        match self.with_no_errors(|this|
                            this.resolve_path(expr.id, path, TypeNS, false)) {
                            Some((DefTy(struct_id, _), _))
                              if self.structs.contains_key(&struct_id) => {
                                self.resolve_error(expr.span,
                                        format!("`{}` is a structure name, but \
                                                 this expression \
                                                 uses it like a function name",
                                                wrong_name).as_slice());

                                self.session.span_help(expr.span,
                                    format!("Did you mean to write: \
                                            `{} {{ /* fields */ }}`?",
                                            wrong_name).as_slice());

                            }
                            _ => {
                                let mut method_scope = false;
                                self.value_ribs.iter().rev().all(|rib| {
                                    let res = match *rib {
                                        Rib { bindings: _, kind: MethodRibKind(_, _) } => true,
                                        Rib { bindings: _, kind: ItemRibKind } => false,
                                        _ => return true, // Keep advancing
                                    };

                                    method_scope = res;
                                    false // Stop advancing
                                });

                                if method_scope && token::get_name(self.self_name).get()
                                                                   == wrong_name {
                                        self.resolve_error(
                                            expr.span,
                                            "`self` is not available \
                                             in a static method. Maybe a \
                                             `self` argument is missing?");
                                } else {
                                    let last_name = path.segments.last().unwrap().identifier.name;
                                    let mut msg = match self.find_fallback_in_self_type(last_name) {
                                        NoSuggestion => {
                                            // limit search to 5 to reduce the number
                                            // of stupid suggestions
                                            self.find_best_match_for_name(wrong_name.as_slice(), 5)
                                                                .map_or("".to_string(),
                                                                        |x| format!("`{}`", x))
                                        }
                                        Field =>
                                            format!("`self.{}`", wrong_name),
                                        Method
                                        | TraitItem =>
                                            format!("to call `self.{}`", wrong_name),
                                        TraitMethod(path_str)
                                        | StaticMethod(path_str) =>
                                            format!("to call `{}::{}`", path_str, wrong_name)
                                    };

                                    if msg.len() > 0 {
                                        msg = format!(". Did you mean {}?", msg)
                                    }

                                    self.resolve_error(
                                        expr.span,
                                        format!("unresolved name `{}`{}",
                                                wrong_name,
                                                msg).as_slice());
                                }
                            }
                        }
                    }
                }

                visit::walk_expr(self, expr);
            }

            ExprClosure(capture_clause, _, ref fn_decl, ref block) => {
                self.capture_mode_map.insert(expr.id, capture_clause);
                self.resolve_function(ClosureRibKind(expr.id, ast::DUMMY_NODE_ID),
                                      Some(&**fn_decl), NoTypeParameters,
                                      &**block);
            }

            ExprProc(ref fn_decl, ref block) => {
                self.capture_mode_map.insert(expr.id, ast::CaptureByValue);
                self.resolve_function(ClosureRibKind(expr.id, block.id),
                                      Some(&**fn_decl), NoTypeParameters,
                                      &**block);
            }

            ExprStruct(ref path, _, _) => {
                // Resolve the path to the structure it goes to. We don't
                // check to ensure that the path is actually a structure; that
                // is checked later during typeck.
                match self.resolve_path(expr.id, path, TypeNS, false) {
                    Some(definition) => self.record_def(expr.id, definition),
                    result => {
                        debug!("(resolving expression) didn't find struct \
                                def: {}", result);
                        let msg = format!("`{}` does not name a structure",
                                          self.path_names_to_string(path));
                        self.resolve_error(path.span, msg.as_slice());
                    }
                }

                visit::walk_expr(self, expr);
            }

            ExprLoop(_, Some(label)) | ExprWhile(_, _, Some(label)) => {
                self.with_label_rib(|this| {
                    let def_like = DlDef(DefLabel(expr.id));

                    {
                        let rib = this.label_ribs.last_mut().unwrap();
                        let renamed = mtwt::resolve(label);
                        rib.bindings.insert(renamed, def_like);
                    }

                    visit::walk_expr(this, expr);
                })
            }

            ExprForLoop(ref pattern, ref head, ref body, optional_label) => {
                self.resolve_expr(&**head);

                self.value_ribs.push(Rib::new(NormalRibKind));

                self.resolve_pattern(&**pattern,
                                     LocalIrrefutableMode,
                                     &mut HashMap::new());

                match optional_label {
                    None => {}
                    Some(label) => {
                        self.label_ribs
                            .push(Rib::new(NormalRibKind));
                        let def_like = DlDef(DefLabel(expr.id));

                        {
                            let rib = self.label_ribs.last_mut().unwrap();
                            let renamed = mtwt::resolve(label);
                            rib.bindings.insert(renamed, def_like);
                        }
                    }
                }

                self.resolve_block(&**body);

                if optional_label.is_some() {
                    drop(self.label_ribs.pop())
                }

                self.value_ribs.pop();
            }

            ExprBreak(Some(label)) | ExprAgain(Some(label)) => {
                let renamed = mtwt::resolve(label);
                match self.search_ribs(self.label_ribs.as_slice(),
                                       renamed, expr.span) {
                    None => {
                        self.resolve_error(
                            expr.span,
                            format!("use of undeclared label `{}`",
                                    token::get_ident(label)).as_slice())
                    }
                    Some(DlDef(def @ DefLabel(_))) => {
                        // Since this def is a label, it is never read.
                        self.record_def(expr.id, (def, LastMod(AllPublic)))
                    }
                    Some(_) => {
                        self.session.span_bug(expr.span,
                                              "label wasn't mapped to a \
                                               label def!")
                    }
                }
            }

            _ => {
                visit::walk_expr(self, expr);
            }
        }
    }

    fn record_candidate_traits_for_expr_if_necessary(&mut self, expr: &Expr) {
        match expr.node {
            ExprField(_, ident) => {
                // FIXME(#6890): Even though you can't treat a method like a
                // field, we need to add any trait methods we find that match
                // the field name so that we can do some nice error reporting
                // later on in typeck.
                let traits = self.search_for_traits_containing_method(ident.node.name);
                self.trait_map.insert(expr.id, traits);
            }
            ExprMethodCall(ident, _, _) => {
                debug!("(recording candidate traits for expr) recording \
                        traits for {}",
                       expr.id);
                let traits = self.search_for_traits_containing_method(ident.node.name);
                self.trait_map.insert(expr.id, traits);
            }
            _ => {
                // Nothing to do.
            }
        }
    }

    fn search_for_traits_containing_method(&mut self, name: Name) -> Vec<DefId> {
        debug!("(searching for traits containing method) looking for '{}'",
               token::get_name(name));

        fn add_trait_info(found_traits: &mut Vec<DefId>,
                          trait_def_id: DefId,
                          name: Name) {
            debug!("(adding trait info) found trait {}:{} for method '{}'",
                trait_def_id.krate,
                trait_def_id.node,
                token::get_name(name));
            found_traits.push(trait_def_id);
        }

        let mut found_traits = Vec::new();
        let mut search_module = self.current_module.clone();
        loop {
            // Look for the current trait.
            match self.current_trait_ref {
                Some((trait_def_id, _)) => {
                    if self.trait_item_map.contains_key(&(name, trait_def_id)) {
                        add_trait_info(&mut found_traits, trait_def_id, name);
                    }
                }
                None => {} // Nothing to do.
            }

            // Look for trait children.
            self.populate_module_if_necessary(&search_module);

            {
                for (_, child_names) in search_module.children.borrow().iter() {
                    let def = match child_names.def_for_namespace(TypeNS) {
                        Some(def) => def,
                        None => continue
                    };
                    let trait_def_id = match def {
                        DefTrait(trait_def_id) => trait_def_id,
                        _ => continue,
                    };
                    if self.trait_item_map.contains_key(&(name, trait_def_id)) {
                        add_trait_info(&mut found_traits, trait_def_id, name);
                    }
                }
            }

            // Look for imports.
            for (_, import) in search_module.import_resolutions.borrow().iter() {
                let target = match import.target_for_namespace(TypeNS) {
                    None => continue,
                    Some(target) => target,
                };
                let did = match target.bindings.def_for_namespace(TypeNS) {
                    Some(DefTrait(trait_def_id)) => trait_def_id,
                    Some(..) | None => continue,
                };
                if self.trait_item_map.contains_key(&(name, did)) {
                    add_trait_info(&mut found_traits, did, name);
                    self.used_imports.insert((import.type_id, TypeNS));
                    if let Some(DefId{krate: kid, ..}) = target.target_module.def_id.get() {
                        self.used_crates.insert(kid);
                    }
                }
            }

            match search_module.parent_link.clone() {
                NoParentLink | ModuleParentLink(..) => break,
                BlockParentLink(parent_module, _) => {
                    search_module = parent_module.upgrade().unwrap();
                }
            }
        }

        found_traits
    }

    fn record_def(&mut self, node_id: NodeId, (def, lp): (Def, LastPrivate)) {
        debug!("(recording def) recording {} for {}, last private {}",
                def, node_id, lp);
        assert!(match lp {LastImport{..} => false, _ => true},
                "Import should only be used for `use` directives");
        self.last_private.insert(node_id, lp);

        match self.def_map.borrow_mut().entry(node_id) {
            // Resolve appears to "resolve" the same ID multiple
            // times, so here is a sanity check it at least comes to
            // the same conclusion! - nmatsakis
            Occupied(entry) => if def != *entry.get() {
                self.session
                    .bug(format!("node_id {} resolved first to {} and \
                                  then {}",
                                 node_id,
                                 *entry.get(),
                                 def).as_slice());
            },
            Vacant(entry) => { entry.set(def); },
        }
    }

    fn enforce_default_binding_mode(&mut self,
                                        pat: &Pat,
                                        pat_binding_mode: BindingMode,
                                        descr: &str) {
        match pat_binding_mode {
            BindByValue(_) => {}
            BindByRef(..) => {
                self.resolve_error(pat.span,
                                   format!("cannot use `ref` binding mode \
                                            with {}",
                                           descr).as_slice());
            }
        }
    }

    //
    // Unused import checking
    //
    // Although this is mostly a lint pass, it lives in here because it depends on
    // resolve data structures and because it finalises the privacy information for
    // `use` directives.
    //

    fn check_for_unused_imports(&mut self, krate: &ast::Crate) {
        let mut visitor = UnusedImportCheckVisitor{ resolver: self };
        visit::walk_crate(&mut visitor, krate);
    }

    fn check_for_item_unused_imports(&mut self, vi: &ViewItem) {
        // Ignore is_public import statements because there's no way to be sure
        // whether they're used or not. Also ignore imports with a dummy span
        // because this means that they were generated in some fashion by the
        // compiler and we don't need to consider them.
        if vi.vis == Public { return }
        if vi.span == DUMMY_SP { return }

        match vi.node {
            ViewItemExternCrate(_, _, id) => {
                if let Some(crate_num) = self.session.cstore.find_extern_mod_stmt_cnum(id) {
                    if !self.used_crates.contains(&crate_num) {
                        self.session.add_lint(lint::builtin::UNUSED_EXTERN_CRATES,
                                              id,
                                              vi.span,
                                              "unused extern crate".to_string());
                    }
                }
            },
            ViewItemUse(ref p) => {
                match p.node {
                    ViewPathSimple(_, _, id) => self.finalize_import(id, p.span),

                    ViewPathList(_, ref list, _) => {
                        for i in list.iter() {
                            self.finalize_import(i.node.id(), i.span);
                        }
                    },
                    ViewPathGlob(_, id) => {
                        if !self.used_imports.contains(&(id, TypeNS)) &&
                           !self.used_imports.contains(&(id, ValueNS)) {
                            self.session
                                .add_lint(lint::builtin::UNUSED_IMPORTS,
                                          id,
                                          p.span,
                                          "unused import".to_string());
                        }
                    },
                }
            }
        }
    }

    // We have information about whether `use` (import) directives are actually used now.
    // If an import is not used at all, we signal a lint error. If an import is only used
    // for a single namespace, we remove the other namespace from the recorded privacy
    // information. That means in privacy.rs, we will only check imports and namespaces
    // which are used. In particular, this means that if an import could name either a
    // public or private item, we will check the correct thing, dependent on how the import
    // is used.
    fn finalize_import(&mut self, id: NodeId, span: Span) {
        debug!("finalizing import uses for {}",
               self.session.codemap().span_to_snippet(span));

        if !self.used_imports.contains(&(id, TypeNS)) &&
           !self.used_imports.contains(&(id, ValueNS)) {
            self.session.add_lint(lint::builtin::UNUSED_IMPORTS,
                                  id,
                                  span,
                                  "unused import".to_string());
        }

        let (v_priv, t_priv) = match self.last_private.get(&id) {
            Some(&LastImport {
                value_priv: v,
                value_used: _,
                type_priv: t,
                type_used: _
            }) => (v, t),
            Some(_) => {
                panic!("we should only have LastImport for `use` directives")
            }
            _ => return,
        };

        let mut v_used = if self.used_imports.contains(&(id, ValueNS)) {
            Used
        } else {
            Unused
        };
        let t_used = if self.used_imports.contains(&(id, TypeNS)) {
            Used
        } else {
            Unused
        };

        match (v_priv, t_priv) {
            // Since some items may be both in the value _and_ type namespaces (e.g., structs)
            // we might have two LastPrivates pointing at the same thing. There is no point
            // checking both, so lets not check the value one.
            (Some(DependsOn(def_v)), Some(DependsOn(def_t))) if def_v == def_t => v_used = Unused,
            _ => {},
        }

        self.last_private.insert(id, LastImport{value_priv: v_priv,
                                                value_used: v_used,
                                                type_priv: t_priv,
                                                type_used: t_used});
    }

    //
    // Diagnostics
    //
    // Diagnostics are not particularly efficient, because they're rarely
    // hit.
    //

    /// A somewhat inefficient routine to obtain the name of a module.
    fn module_to_string(&self, module: &Module) -> String {
        let mut names = Vec::new();

        fn collect_mod(names: &mut Vec<ast::Name>, module: &Module) {
            match module.parent_link {
                NoParentLink => {}
                ModuleParentLink(ref module, name) => {
                    names.push(name);
                    collect_mod(names, &*module.upgrade().unwrap());
                }
                BlockParentLink(ref module, _) => {
                    // danger, shouldn't be ident?
                    names.push(special_idents::opaque.name);
                    collect_mod(names, &*module.upgrade().unwrap());
                }
            }
        }
        collect_mod(&mut names, module);

        if names.len() == 0 {
            return "???".to_string();
        }
        self.names_to_string(names.into_iter().rev()
                                  .collect::<Vec<ast::Name>>()
                                  .as_slice())
    }

    #[allow(dead_code)]   // useful for debugging
    fn dump_module(&mut self, module_: Rc<Module>) {
        debug!("Dump of module `{}`:", self.module_to_string(&*module_));

        debug!("Children:");
        self.populate_module_if_necessary(&module_);
        for (&name, _) in module_.children.borrow().iter() {
            debug!("* {}", token::get_name(name));
        }

        debug!("Import resolutions:");
        let import_resolutions = module_.import_resolutions.borrow();
        for (&name, import_resolution) in import_resolutions.iter() {
            let value_repr;
            match import_resolution.target_for_namespace(ValueNS) {
                None => { value_repr = "".to_string(); }
                Some(_) => {
                    value_repr = " value:?".to_string();
                    // FIXME #4954
                }
            }

            let type_repr;
            match import_resolution.target_for_namespace(TypeNS) {
                None => { type_repr = "".to_string(); }
                Some(_) => {
                    type_repr = " type:?".to_string();
                    // FIXME #4954
                }
            }

            debug!("* {}:{}{}", token::get_name(name), value_repr, type_repr);
        }
    }
}

pub struct CrateMap {
    pub def_map: DefMap,
    pub freevars: RefCell<FreevarMap>,
    pub capture_mode_map: RefCell<CaptureModeMap>,
    pub exp_map2: ExportMap2,
    pub trait_map: TraitMap,
    pub external_exports: ExternalExports,
    pub last_private_map: LastPrivateMap,
}

/// Entry point to crate resolution.
pub fn resolve_crate(session: &Session,
                     _: &LanguageItems,
                     krate: &Crate)
                  -> CrateMap {
    let mut resolver = Resolver::new(session, krate.span);
    resolver.resolve(krate);
    CrateMap {
        def_map: resolver.def_map,
        freevars: resolver.freevars,
        capture_mode_map: RefCell::new(resolver.capture_mode_map),
        exp_map2: resolver.export_map2,
        trait_map: resolver.trait_map,
        external_exports: resolver.external_exports,
        last_private_map: resolver.last_private,
    }
}
