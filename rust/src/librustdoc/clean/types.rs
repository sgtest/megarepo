use std::cell::RefCell;
use std::default::Default;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::iter::FromIterator;
use std::lazy::SyncOnceCell as OnceCell;
use std::rc::Rc;
use std::sync::Arc;
use std::{slice, vec};

use rustc_ast::attr;
use rustc_ast::util::comments::beautify_doc_string;
use rustc_ast::{self as ast, AttrStyle};
use rustc_ast::{FloatTy, IntTy, UintTy};
use rustc_attr::{ConstStability, Stability, StabilityLevel};
use rustc_data_structures::fx::{FxHashMap, FxHashSet};
use rustc_feature::UnstableFeatures;
use rustc_hir as hir;
use rustc_hir::def::Res;
use rustc_hir::def_id::{CrateNum, DefId, LOCAL_CRATE};
use rustc_hir::lang_items::LangItem;
use rustc_hir::Mutability;
use rustc_index::vec::IndexVec;
use rustc_middle::ty::{AssocKind, TyCtxt};
use rustc_span::hygiene::MacroKind;
use rustc_span::source_map::DUMMY_SP;
use rustc_span::symbol::{kw, sym, Ident, Symbol, SymbolStr};
use rustc_span::{self, FileName};
use rustc_target::abi::VariantIdx;
use rustc_target::spec::abi::Abi;
use smallvec::{smallvec, SmallVec};

use crate::clean::cfg::Cfg;
use crate::clean::external_path;
use crate::clean::inline;
use crate::clean::types::Type::{QPath, ResolvedPath};
use crate::clean::Clean;
use crate::core::DocContext;
use crate::doctree;
use crate::formats::cache::cache;
use crate::formats::item_type::ItemType;
use crate::html::render::cache::ExternalLocation;

use self::FnRetTy::*;
use self::ItemKind::*;
use self::SelfTy::*;
use self::Type::*;

thread_local!(crate static MAX_DEF_ID: RefCell<FxHashMap<CrateNum, DefId>> = Default::default());

#[derive(Clone, Debug)]
crate struct Crate {
    crate name: String,
    crate version: Option<String>,
    crate src: FileName,
    crate module: Option<Item>,
    crate externs: Vec<(CrateNum, ExternalCrate)>,
    crate primitives: Vec<(DefId, PrimitiveType)>,
    // These are later on moved into `CACHEKEY`, leaving the map empty.
    // Only here so that they can be filtered through the rustdoc passes.
    crate external_traits: Rc<RefCell<FxHashMap<DefId, Trait>>>,
    crate masked_crates: FxHashSet<CrateNum>,
    crate collapsed: bool,
}

#[derive(Clone, Debug)]
crate struct ExternalCrate {
    crate name: String,
    crate src: FileName,
    crate attrs: Attributes,
    crate primitives: Vec<(DefId, PrimitiveType)>,
    crate keywords: Vec<(DefId, String)>,
}

/// Anything with a source location and set of attributes and, optionally, a
/// name. That is, anything that can be documented. This doesn't correspond
/// directly to the AST's concept of an item; it's a strict superset.
#[derive(Clone)]
crate struct Item {
    /// Stringified span
    crate source: Span,
    /// Not everything has a name. E.g., impls
    crate name: Option<String>,
    crate attrs: Attributes,
    crate visibility: Visibility,
    crate kind: ItemKind,
    crate def_id: DefId,
    crate stability: Option<Stability>,
    crate deprecation: Option<Deprecation>,
    crate const_stability: Option<ConstStability>,
}

impl fmt::Debug for Item {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let def_id: &dyn fmt::Debug = if self.is_fake() { &"**FAKE**" } else { &self.def_id };

        fmt.debug_struct("Item")
            .field("source", &self.source)
            .field("name", &self.name)
            .field("attrs", &self.attrs)
            .field("kind", &self.kind)
            .field("visibility", &self.visibility)
            .field("def_id", def_id)
            .field("stability", &self.stability)
            .field("deprecation", &self.deprecation)
            .finish()
    }
}

impl Item {
    /// Finds the `doc` attribute as a NameValue and returns the corresponding
    /// value found.
    crate fn doc_value(&self) -> Option<&str> {
        self.attrs.doc_value()
    }

    /// Convenience wrapper around [`Self::from_def_id_and_parts`] which converts
    /// `hir_id` to a [`DefId`]
    pub fn from_hir_id_and_parts(
        hir_id: hir::HirId,
        name: Option<Symbol>,
        kind: ItemKind,
        cx: &DocContext<'_>,
    ) -> Item {
        Item::from_def_id_and_parts(
            cx.tcx.hir().local_def_id(hir_id).to_def_id(),
            name.clean(cx),
            kind,
            cx,
        )
    }

    pub fn from_def_id_and_parts(
        def_id: DefId,
        name: Option<String>,
        kind: ItemKind,
        cx: &DocContext<'_>,
    ) -> Item {
        debug!("name={:?}, def_id={:?}", name, def_id);

        // `span_if_local()` lies about functions and only gives the span of the function signature
        let source = def_id.as_local().map_or_else(
            || cx.tcx.def_span(def_id),
            |local| {
                let hir = cx.tcx.hir();
                hir.span_with_body(hir.local_def_id_to_hir_id(local))
            },
        );

        Item {
            def_id,
            kind,
            name,
            source: source.clean(cx),
            attrs: cx.tcx.get_attrs(def_id).clean(cx),
            visibility: cx.tcx.visibility(def_id).clean(cx),
            stability: cx.tcx.lookup_stability(def_id).cloned(),
            deprecation: cx.tcx.lookup_deprecation(def_id).clean(cx),
            const_stability: cx.tcx.lookup_const_stability(def_id).cloned(),
        }
    }

    /// Finds all `doc` attributes as NameValues and returns their corresponding values, joined
    /// with newlines.
    crate fn collapsed_doc_value(&self) -> Option<String> {
        self.attrs.collapsed_doc_value()
    }

    crate fn links(&self) -> Vec<RenderedLink> {
        self.attrs.links(&self.def_id.krate)
    }

    crate fn is_crate(&self) -> bool {
        match self.kind {
            StrippedItem(box ModuleItem(Module { is_crate: true, .. }))
            | ModuleItem(Module { is_crate: true, .. }) => true,
            _ => false,
        }
    }
    crate fn is_mod(&self) -> bool {
        self.type_() == ItemType::Module
    }
    crate fn is_trait(&self) -> bool {
        self.type_() == ItemType::Trait
    }
    crate fn is_struct(&self) -> bool {
        self.type_() == ItemType::Struct
    }
    crate fn is_enum(&self) -> bool {
        self.type_() == ItemType::Enum
    }
    crate fn is_variant(&self) -> bool {
        self.type_() == ItemType::Variant
    }
    crate fn is_associated_type(&self) -> bool {
        self.type_() == ItemType::AssocType
    }
    crate fn is_associated_const(&self) -> bool {
        self.type_() == ItemType::AssocConst
    }
    crate fn is_method(&self) -> bool {
        self.type_() == ItemType::Method
    }
    crate fn is_ty_method(&self) -> bool {
        self.type_() == ItemType::TyMethod
    }
    crate fn is_typedef(&self) -> bool {
        self.type_() == ItemType::Typedef
    }
    crate fn is_primitive(&self) -> bool {
        self.type_() == ItemType::Primitive
    }
    crate fn is_union(&self) -> bool {
        self.type_() == ItemType::Union
    }
    crate fn is_import(&self) -> bool {
        self.type_() == ItemType::Import
    }
    crate fn is_extern_crate(&self) -> bool {
        self.type_() == ItemType::ExternCrate
    }
    crate fn is_keyword(&self) -> bool {
        self.type_() == ItemType::Keyword
    }
    crate fn is_stripped(&self) -> bool {
        match self.kind {
            StrippedItem(..) => true,
            ImportItem(ref i) => !i.should_be_displayed,
            _ => false,
        }
    }
    crate fn has_stripped_fields(&self) -> Option<bool> {
        match self.kind {
            StructItem(ref _struct) => Some(_struct.fields_stripped),
            UnionItem(ref union) => Some(union.fields_stripped),
            VariantItem(Variant { kind: VariantKind::Struct(ref vstruct) }) => {
                Some(vstruct.fields_stripped)
            }
            _ => None,
        }
    }

    crate fn stability_class(&self) -> Option<String> {
        self.stability.as_ref().and_then(|ref s| {
            let mut classes = Vec::with_capacity(2);

            if s.level.is_unstable() {
                classes.push("unstable");
            }

            // FIXME: what about non-staged API items that are deprecated?
            if self.deprecation.is_some() {
                classes.push("deprecated");
            }

            if !classes.is_empty() { Some(classes.join(" ")) } else { None }
        })
    }

    crate fn stable_since(&self) -> Option<SymbolStr> {
        match self.stability?.level {
            StabilityLevel::Stable { since, .. } => Some(since.as_str()),
            StabilityLevel::Unstable { .. } => None,
        }
    }

    crate fn const_stable_since(&self) -> Option<SymbolStr> {
        match self.const_stability?.level {
            StabilityLevel::Stable { since, .. } => Some(since.as_str()),
            StabilityLevel::Unstable { .. } => None,
        }
    }

    crate fn is_non_exhaustive(&self) -> bool {
        self.attrs.other_attrs.iter().any(|a| a.has_name(sym::non_exhaustive))
    }

    /// Returns a documentation-level item type from the item.
    crate fn type_(&self) -> ItemType {
        ItemType::from(self)
    }

    crate fn is_default(&self) -> bool {
        match self.kind {
            ItemKind::MethodItem(_, Some(defaultness)) => {
                defaultness.has_value() && !defaultness.is_final()
            }
            _ => false,
        }
    }

    /// See comments on next_def_id
    crate fn is_fake(&self) -> bool {
        MAX_DEF_ID.with(|m| {
            m.borrow().get(&self.def_id.krate).map(|id| self.def_id >= *id).unwrap_or(false)
        })
    }
}

#[derive(Clone, Debug)]
crate enum ItemKind {
    ExternCrateItem(String, Option<String>),
    ImportItem(Import),
    StructItem(Struct),
    UnionItem(Union),
    EnumItem(Enum),
    FunctionItem(Function),
    ModuleItem(Module),
    TypedefItem(Typedef, bool /* is associated type */),
    OpaqueTyItem(OpaqueTy),
    StaticItem(Static),
    ConstantItem(Constant),
    TraitItem(Trait),
    TraitAliasItem(TraitAlias),
    ImplItem(Impl),
    /// A method signature only. Used for required methods in traits (ie,
    /// non-default-methods).
    TyMethodItem(Function),
    /// A method with a body.
    MethodItem(Function, Option<hir::Defaultness>),
    StructFieldItem(Type),
    VariantItem(Variant),
    /// `fn`s from an extern block
    ForeignFunctionItem(Function),
    /// `static`s from an extern block
    ForeignStaticItem(Static),
    /// `type`s from an extern block
    ForeignTypeItem,
    MacroItem(Macro),
    ProcMacroItem(ProcMacro),
    PrimitiveItem(PrimitiveType),
    AssocConstItem(Type, Option<String>),
    AssocTypeItem(Vec<GenericBound>, Option<Type>),
    /// An item that has been stripped by a rustdoc pass
    StrippedItem(Box<ItemKind>),
    KeywordItem(String),
}

impl ItemKind {
    crate fn is_type_alias(&self) -> bool {
        match *self {
            ItemKind::TypedefItem(_, _) | ItemKind::AssocTypeItem(_, _) => true,
            _ => false,
        }
    }

    crate fn as_assoc_kind(&self) -> Option<AssocKind> {
        match *self {
            ItemKind::AssocConstItem(..) => Some(AssocKind::Const),
            ItemKind::AssocTypeItem(..) => Some(AssocKind::Type),
            ItemKind::TyMethodItem(..) | ItemKind::MethodItem(..) => Some(AssocKind::Fn),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
crate struct Module {
    crate items: Vec<Item>,
    crate is_crate: bool,
}

crate struct ListAttributesIter<'a> {
    attrs: slice::Iter<'a, ast::Attribute>,
    current_list: vec::IntoIter<ast::NestedMetaItem>,
    name: Symbol,
}

impl<'a> Iterator for ListAttributesIter<'a> {
    type Item = ast::NestedMetaItem;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(nested) = self.current_list.next() {
            return Some(nested);
        }

        for attr in &mut self.attrs {
            if let Some(list) = attr.meta_item_list() {
                if attr.has_name(self.name) {
                    self.current_list = list.into_iter();
                    if let Some(nested) = self.current_list.next() {
                        return Some(nested);
                    }
                }
            }
        }

        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let lower = self.current_list.len();
        (lower, None)
    }
}

crate trait AttributesExt {
    /// Finds an attribute as List and returns the list of attributes nested inside.
    fn lists(&self, name: Symbol) -> ListAttributesIter<'_>;
}

impl AttributesExt for [ast::Attribute] {
    fn lists(&self, name: Symbol) -> ListAttributesIter<'_> {
        ListAttributesIter { attrs: self.iter(), current_list: Vec::new().into_iter(), name }
    }
}

crate trait NestedAttributesExt {
    /// Returns `true` if the attribute list contains a specific `Word`
    fn has_word(self, word: Symbol) -> bool;
}

impl<I: IntoIterator<Item = ast::NestedMetaItem>> NestedAttributesExt for I {
    fn has_word(self, word: Symbol) -> bool {
        self.into_iter().any(|attr| attr.is_word() && attr.has_name(word))
    }
}

/// A portion of documentation, extracted from a `#[doc]` attribute.
///
/// Each variant contains the line number within the complete doc-comment where the fragment
/// starts, as well as the Span where the corresponding doc comment or attribute is located.
///
/// Included files are kept separate from inline doc comments so that proper line-number
/// information can be given when a doctest fails. Sugared doc comments and "raw" doc comments are
/// kept separate because of issue #42760.
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
crate struct DocFragment {
    crate line: usize,
    crate span: rustc_span::Span,
    /// The module this doc-comment came from.
    ///
    /// This allows distinguishing between the original documentation and a pub re-export.
    /// If it is `None`, the item was not re-exported.
    crate parent_module: Option<DefId>,
    crate doc: String,
    crate kind: DocFragmentKind,
}

#[derive(Clone, PartialEq, Eq, Debug, Hash)]
crate enum DocFragmentKind {
    /// A doc fragment created from a `///` or `//!` doc comment.
    SugaredDoc,
    /// A doc fragment created from a "raw" `#[doc=""]` attribute.
    RawDoc,
    /// A doc fragment created from a `#[doc(include="filename")]` attribute. Contains both the
    /// given filename and the file contents.
    Include { filename: String },
}

impl<'a> FromIterator<&'a DocFragment> for String {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = &'a DocFragment>,
    {
        iter.into_iter().fold(String::new(), |mut acc, frag| {
            if !acc.is_empty() {
                acc.push('\n');
            }
            acc.push_str(&frag.doc);
            acc
        })
    }
}

#[derive(Clone, Debug, Default)]
crate struct Attributes {
    crate doc_strings: Vec<DocFragment>,
    crate other_attrs: Vec<ast::Attribute>,
    crate cfg: Option<Arc<Cfg>>,
    crate span: Option<rustc_span::Span>,
    /// map from Rust paths to resolved defs and potential URL fragments
    crate links: Vec<ItemLink>,
    crate inner_docs: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
/// A link that has not yet been rendered.
///
/// This link will be turned into a rendered link by [`Attributes::links`]
crate struct ItemLink {
    /// The original link written in the markdown
    pub(crate) link: String,
    /// The link text displayed in the HTML.
    ///
    /// This may not be the same as `link` if there was a disambiguator
    /// in an intra-doc link (e.g. \[`fn@f`\])
    pub(crate) link_text: String,
    pub(crate) did: Option<DefId>,
    /// The url fragment to append to the link
    pub(crate) fragment: Option<String>,
}

pub struct RenderedLink {
    /// The text the link was original written as.
    ///
    /// This could potentially include disambiguators and backticks.
    pub(crate) original_text: String,
    /// The text to display in the HTML
    pub(crate) new_text: String,
    /// The URL to put in the `href`
    pub(crate) href: String,
}

impl Attributes {
    /// Extracts the content from an attribute `#[doc(cfg(content))]`.
    crate fn extract_cfg(mi: &ast::MetaItem) -> Option<&ast::MetaItem> {
        use rustc_ast::NestedMetaItem::MetaItem;

        if let ast::MetaItemKind::List(ref nmis) = mi.kind {
            if nmis.len() == 1 {
                if let MetaItem(ref cfg_mi) = nmis[0] {
                    if cfg_mi.has_name(sym::cfg) {
                        if let ast::MetaItemKind::List(ref cfg_nmis) = cfg_mi.kind {
                            if cfg_nmis.len() == 1 {
                                if let MetaItem(ref content_mi) = cfg_nmis[0] {
                                    return Some(content_mi);
                                }
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Reads a `MetaItem` from within an attribute, looks for whether it is a
    /// `#[doc(include="file")]`, and returns the filename and contents of the file as loaded from
    /// its expansion.
    crate fn extract_include(mi: &ast::MetaItem) -> Option<(String, String)> {
        mi.meta_item_list().and_then(|list| {
            for meta in list {
                if meta.has_name(sym::include) {
                    // the actual compiled `#[doc(include="filename")]` gets expanded to
                    // `#[doc(include(file="filename", contents="file contents")]` so we need to
                    // look for that instead
                    return meta.meta_item_list().and_then(|list| {
                        let mut filename: Option<String> = None;
                        let mut contents: Option<String> = None;

                        for it in list {
                            if it.has_name(sym::file) {
                                if let Some(name) = it.value_str() {
                                    filename = Some(name.to_string());
                                }
                            } else if it.has_name(sym::contents) {
                                if let Some(docs) = it.value_str() {
                                    contents = Some(docs.to_string());
                                }
                            }
                        }

                        if let (Some(filename), Some(contents)) = (filename, contents) {
                            Some((filename, contents))
                        } else {
                            None
                        }
                    });
                }
            }

            None
        })
    }

    crate fn has_doc_flag(&self, flag: Symbol) -> bool {
        for attr in &self.other_attrs {
            if !attr.has_name(sym::doc) {
                continue;
            }

            if let Some(items) = attr.meta_item_list() {
                if items.iter().filter_map(|i| i.meta_item()).any(|it| it.has_name(flag)) {
                    return true;
                }
            }
        }

        false
    }

    crate fn from_ast(
        diagnostic: &::rustc_errors::Handler,
        attrs: &[ast::Attribute],
        additional_attrs: Option<(&[ast::Attribute], DefId)>,
    ) -> Attributes {
        let mut doc_strings = vec![];
        let mut sp = None;
        let mut cfg = Cfg::True;
        let mut doc_line = 0;

        let clean_attr = |(attr, parent_module): (&ast::Attribute, _)| {
            if let Some(value) = attr.doc_str() {
                trace!("got doc_str={:?}", value);
                let value = beautify_doc_string(value);
                let kind = if attr.is_doc_comment() {
                    DocFragmentKind::SugaredDoc
                } else {
                    DocFragmentKind::RawDoc
                };

                let line = doc_line;
                doc_line += value.lines().count();
                doc_strings.push(DocFragment {
                    line,
                    span: attr.span,
                    doc: value,
                    kind,
                    parent_module,
                });

                if sp.is_none() {
                    sp = Some(attr.span);
                }
                None
            } else {
                if attr.has_name(sym::doc) {
                    if let Some(mi) = attr.meta() {
                        if let Some(cfg_mi) = Attributes::extract_cfg(&mi) {
                            // Extracted #[doc(cfg(...))]
                            match Cfg::parse(cfg_mi) {
                                Ok(new_cfg) => cfg &= new_cfg,
                                Err(e) => diagnostic.span_err(e.span, e.msg),
                            }
                        } else if let Some((filename, contents)) = Attributes::extract_include(&mi)
                        {
                            let line = doc_line;
                            doc_line += contents.lines().count();
                            doc_strings.push(DocFragment {
                                line,
                                span: attr.span,
                                doc: contents,
                                kind: DocFragmentKind::Include { filename },
                                parent_module: parent_module,
                            });
                        }
                    }
                }
                Some(attr.clone())
            }
        };

        // Additional documentation should be shown before the original documentation
        let other_attrs = additional_attrs
            .into_iter()
            .map(|(attrs, id)| attrs.iter().map(move |attr| (attr, Some(id))))
            .flatten()
            .chain(attrs.iter().map(|attr| (attr, None)))
            .filter_map(clean_attr)
            .collect();

        // treat #[target_feature(enable = "feat")] attributes as if they were
        // #[doc(cfg(target_feature = "feat"))] attributes as well
        for attr in attrs.lists(sym::target_feature) {
            if attr.has_name(sym::enable) {
                if let Some(feat) = attr.value_str() {
                    let meta = attr::mk_name_value_item_str(
                        Ident::with_dummy_span(sym::target_feature),
                        feat,
                        DUMMY_SP,
                    );
                    if let Ok(feat_cfg) = Cfg::parse(&meta) {
                        cfg &= feat_cfg;
                    }
                }
            }
        }

        let inner_docs = attrs
            .iter()
            .find(|a| a.doc_str().is_some())
            .map_or(true, |a| a.style == AttrStyle::Inner);

        Attributes {
            doc_strings,
            other_attrs,
            cfg: if cfg == Cfg::True { None } else { Some(Arc::new(cfg)) },
            span: sp,
            links: vec![],
            inner_docs,
        }
    }

    /// Finds the `doc` attribute as a NameValue and returns the corresponding
    /// value found.
    crate fn doc_value(&self) -> Option<&str> {
        self.doc_strings.first().map(|s| s.doc.as_str())
    }

    /// Finds all `doc` attributes as NameValues and returns their corresponding values, joined
    /// with newlines.
    crate fn collapsed_doc_value(&self) -> Option<String> {
        if !self.doc_strings.is_empty() { Some(self.doc_strings.iter().collect()) } else { None }
    }

    /// Gets links as a vector
    ///
    /// Cache must be populated before call
    crate fn links(&self, krate: &CrateNum) -> Vec<RenderedLink> {
        use crate::html::format::href;
        use crate::html::render::CURRENT_DEPTH;

        self.links
            .iter()
            .filter_map(|ItemLink { link: s, link_text, did, fragment }| {
                match *did {
                    Some(did) => {
                        if let Some((mut href, ..)) = href(did) {
                            if let Some(ref fragment) = *fragment {
                                href.push_str("#");
                                href.push_str(fragment);
                            }
                            Some(RenderedLink {
                                original_text: s.clone(),
                                new_text: link_text.clone(),
                                href,
                            })
                        } else {
                            None
                        }
                    }
                    None => {
                        if let Some(ref fragment) = *fragment {
                            let cache = cache();
                            let url = match cache.extern_locations.get(krate) {
                                Some(&(_, _, ExternalLocation::Local)) => {
                                    let depth = CURRENT_DEPTH.with(|l| l.get());
                                    "../".repeat(depth)
                                }
                                Some(&(_, _, ExternalLocation::Remote(ref s))) => s.to_string(),
                                Some(&(_, _, ExternalLocation::Unknown)) | None => String::from(
                                    // NOTE: intentionally doesn't pass crate name to avoid having
                                    // different primitive links between crates
                                    if UnstableFeatures::from_environment(None).is_nightly_build() {
                                        "https://doc.rust-lang.org/nightly"
                                    } else {
                                        "https://doc.rust-lang.org"
                                    },
                                ),
                            };
                            // This is a primitive so the url is done "by hand".
                            let tail = fragment.find('#').unwrap_or_else(|| fragment.len());
                            Some(RenderedLink {
                                original_text: s.clone(),
                                new_text: link_text.clone(),
                                href: format!(
                                    "{}{}std/primitive.{}.html{}",
                                    url,
                                    if !url.ends_with('/') { "/" } else { "" },
                                    &fragment[..tail],
                                    &fragment[tail..]
                                ),
                            })
                        } else {
                            panic!("This isn't a primitive?!");
                        }
                    }
                }
            })
            .collect()
    }

    crate fn get_doc_aliases(&self) -> FxHashSet<String> {
        self.other_attrs
            .lists(sym::doc)
            .filter(|a| a.has_name(sym::alias))
            .filter_map(|a| a.value_str().map(|s| s.to_string()))
            .filter(|v| !v.is_empty())
            .collect::<FxHashSet<_>>()
    }
}

impl PartialEq for Attributes {
    fn eq(&self, rhs: &Self) -> bool {
        self.doc_strings == rhs.doc_strings
            && self.cfg == rhs.cfg
            && self.span == rhs.span
            && self.links == rhs.links
            && self
                .other_attrs
                .iter()
                .map(|attr| attr.id)
                .eq(rhs.other_attrs.iter().map(|attr| attr.id))
    }
}

impl Eq for Attributes {}

impl Hash for Attributes {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.doc_strings.hash(hasher);
        self.cfg.hash(hasher);
        self.span.hash(hasher);
        self.links.hash(hasher);
        for attr in &self.other_attrs {
            attr.id.hash(hasher);
        }
    }
}

impl AttributesExt for Attributes {
    fn lists(&self, name: Symbol) -> ListAttributesIter<'_> {
        self.other_attrs.lists(name)
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Hash)]
crate enum GenericBound {
    TraitBound(PolyTrait, hir::TraitBoundModifier),
    Outlives(Lifetime),
}

impl GenericBound {
    crate fn maybe_sized(cx: &DocContext<'_>) -> GenericBound {
        let did = cx.tcx.require_lang_item(LangItem::Sized, None);
        let empty = cx.tcx.intern_substs(&[]);
        let path = external_path(cx, cx.tcx.item_name(did), Some(did), false, vec![], empty);
        inline::record_extern_fqn(cx, did, TypeKind::Trait);
        GenericBound::TraitBound(
            PolyTrait {
                trait_: ResolvedPath { path, param_names: None, did, is_generic: false },
                generic_params: Vec::new(),
            },
            hir::TraitBoundModifier::Maybe,
        )
    }

    crate fn is_sized_bound(&self, cx: &DocContext<'_>) -> bool {
        use rustc_hir::TraitBoundModifier as TBM;
        if let GenericBound::TraitBound(PolyTrait { ref trait_, .. }, TBM::None) = *self {
            if trait_.def_id() == cx.tcx.lang_items().sized_trait() {
                return true;
            }
        }
        false
    }

    crate fn get_poly_trait(&self) -> Option<PolyTrait> {
        if let GenericBound::TraitBound(ref p, _) = *self {
            return Some(p.clone());
        }
        None
    }

    crate fn get_trait_type(&self) -> Option<Type> {
        if let GenericBound::TraitBound(PolyTrait { ref trait_, .. }, _) = *self {
            Some(trait_.clone())
        } else {
            None
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Hash)]
crate struct Lifetime(pub String);

impl Lifetime {
    crate fn get_ref<'a>(&'a self) -> &'a str {
        let Lifetime(ref s) = *self;
        let s: &'a str = s;
        s
    }

    crate fn statik() -> Lifetime {
        Lifetime("'static".to_string())
    }

    crate fn elided() -> Lifetime {
        Lifetime("'_".to_string())
    }
}

#[derive(Clone, Debug)]
crate enum WherePredicate {
    BoundPredicate { ty: Type, bounds: Vec<GenericBound> },
    RegionPredicate { lifetime: Lifetime, bounds: Vec<GenericBound> },
    EqPredicate { lhs: Type, rhs: Type },
}

impl WherePredicate {
    crate fn get_bounds(&self) -> Option<&[GenericBound]> {
        match *self {
            WherePredicate::BoundPredicate { ref bounds, .. } => Some(bounds),
            WherePredicate::RegionPredicate { ref bounds, .. } => Some(bounds),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Hash)]
crate enum GenericParamDefKind {
    Lifetime,
    Type {
        did: DefId,
        bounds: Vec<GenericBound>,
        default: Option<Type>,
        synthetic: Option<hir::SyntheticTyParamKind>,
    },
    Const {
        did: DefId,
        ty: Type,
    },
}

impl GenericParamDefKind {
    crate fn is_type(&self) -> bool {
        match *self {
            GenericParamDefKind::Type { .. } => true,
            _ => false,
        }
    }

    // FIXME(eddyb) this either returns the default of a type parameter, or the
    // type of a `const` parameter. It seems that the intention is to *visit*
    // any embedded types, but `get_type` seems to be the wrong name for that.
    crate fn get_type(&self) -> Option<Type> {
        match self {
            GenericParamDefKind::Type { default, .. } => default.clone(),
            GenericParamDefKind::Const { ty, .. } => Some(ty.clone()),
            GenericParamDefKind::Lifetime => None,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Hash)]
crate struct GenericParamDef {
    crate name: String,
    crate kind: GenericParamDefKind,
}

impl GenericParamDef {
    crate fn is_synthetic_type_param(&self) -> bool {
        match self.kind {
            GenericParamDefKind::Lifetime | GenericParamDefKind::Const { .. } => false,
            GenericParamDefKind::Type { ref synthetic, .. } => synthetic.is_some(),
        }
    }

    crate fn is_type(&self) -> bool {
        self.kind.is_type()
    }

    crate fn get_type(&self) -> Option<Type> {
        self.kind.get_type()
    }

    crate fn get_bounds(&self) -> Option<&[GenericBound]> {
        match self.kind {
            GenericParamDefKind::Type { ref bounds, .. } => Some(bounds),
            _ => None,
        }
    }
}

// maybe use a Generic enum and use Vec<Generic>?
#[derive(Clone, Debug, Default)]
crate struct Generics {
    crate params: Vec<GenericParamDef>,
    crate where_predicates: Vec<WherePredicate>,
}

#[derive(Clone, Debug)]
crate struct Function {
    crate decl: FnDecl,
    crate generics: Generics,
    crate header: hir::FnHeader,
    crate all_types: Vec<(Type, TypeKind)>,
    crate ret_types: Vec<(Type, TypeKind)>,
}

#[derive(Clone, PartialEq, Eq, Debug, Hash)]
crate struct FnDecl {
    crate inputs: Arguments,
    crate output: FnRetTy,
    crate c_variadic: bool,
    crate attrs: Attributes,
}

impl FnDecl {
    crate fn self_type(&self) -> Option<SelfTy> {
        self.inputs.values.get(0).and_then(|v| v.to_self())
    }

    /// Returns the sugared return type for an async function.
    ///
    /// For example, if the return type is `impl std::future::Future<Output = i32>`, this function
    /// will return `i32`.
    ///
    /// # Panics
    ///
    /// This function will panic if the return type does not match the expected sugaring for async
    /// functions.
    crate fn sugared_async_return_type(&self) -> FnRetTy {
        match &self.output {
            FnRetTy::Return(Type::ImplTrait(bounds)) => match &bounds[0] {
                GenericBound::TraitBound(PolyTrait { trait_, .. }, ..) => {
                    let bindings = trait_.bindings().unwrap();
                    FnRetTy::Return(bindings[0].ty().clone())
                }
                _ => panic!("unexpected desugaring of async function"),
            },
            _ => panic!("unexpected desugaring of async function"),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Hash)]
crate struct Arguments {
    crate values: Vec<Argument>,
}

#[derive(Clone, PartialEq, Eq, Debug, Hash)]
crate struct Argument {
    crate type_: Type,
    crate name: String,
}

#[derive(Clone, PartialEq, Debug)]
crate enum SelfTy {
    SelfValue,
    SelfBorrowed(Option<Lifetime>, Mutability),
    SelfExplicit(Type),
}

impl Argument {
    crate fn to_self(&self) -> Option<SelfTy> {
        if self.name != "self" {
            return None;
        }
        if self.type_.is_self_type() {
            return Some(SelfValue);
        }
        match self.type_ {
            BorrowedRef { ref lifetime, mutability, ref type_ } if type_.is_self_type() => {
                Some(SelfBorrowed(lifetime.clone(), mutability))
            }
            _ => Some(SelfExplicit(self.type_.clone())),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Hash)]
crate enum FnRetTy {
    Return(Type),
    DefaultReturn,
}

impl GetDefId for FnRetTy {
    fn def_id(&self) -> Option<DefId> {
        match *self {
            Return(ref ty) => ty.def_id(),
            DefaultReturn => None,
        }
    }
}

#[derive(Clone, Debug)]
crate struct Trait {
    crate unsafety: hir::Unsafety,
    crate items: Vec<Item>,
    crate generics: Generics,
    crate bounds: Vec<GenericBound>,
    crate is_spotlight: bool,
    crate is_auto: bool,
}

#[derive(Clone, Debug)]
crate struct TraitAlias {
    crate generics: Generics,
    crate bounds: Vec<GenericBound>,
}

/// A trait reference, which may have higher ranked lifetimes.
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
crate struct PolyTrait {
    crate trait_: Type,
    crate generic_params: Vec<GenericParamDef>,
}

/// A representation of a type suitable for hyperlinking purposes. Ideally, one can get the original
/// type out of the AST/`TyCtxt` given one of these, if more information is needed. Most
/// importantly, it does not preserve mutability or boxes.
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
crate enum Type {
    /// Structs/enums/traits (most that would be an `hir::TyKind::Path`).
    ResolvedPath {
        path: Path,
        param_names: Option<Vec<GenericBound>>,
        did: DefId,
        /// `true` if is a `T::Name` path for associated types.
        is_generic: bool,
    },
    /// For parameterized types, so the consumer of the JSON don't go
    /// looking for types which don't exist anywhere.
    Generic(String),
    /// Primitives are the fixed-size numeric types (plus int/usize/float), char,
    /// arrays, slices, and tuples.
    Primitive(PrimitiveType),
    /// `extern "ABI" fn`
    BareFunction(Box<BareFunctionDecl>),
    Tuple(Vec<Type>),
    Slice(Box<Type>),
    Array(Box<Type>, String),
    Never,
    RawPointer(Mutability, Box<Type>),
    BorrowedRef {
        lifetime: Option<Lifetime>,
        mutability: Mutability,
        type_: Box<Type>,
    },

    // `<Type as Trait>::Name`
    QPath {
        name: String,
        self_type: Box<Type>,
        trait_: Box<Type>,
    },

    // `_`
    Infer,

    // `impl TraitA + TraitB + ...`
    ImplTrait(Vec<GenericBound>),
}

#[derive(Clone, PartialEq, Eq, Hash, Copy, Debug)]
crate enum PrimitiveType {
    Isize,
    I8,
    I16,
    I32,
    I64,
    I128,
    Usize,
    U8,
    U16,
    U32,
    U64,
    U128,
    F32,
    F64,
    Char,
    Bool,
    Str,
    Slice,
    Array,
    Tuple,
    Unit,
    RawPointer,
    Reference,
    Fn,
    Never,
}

#[derive(Clone, PartialEq, Eq, Hash, Copy, Debug)]
crate enum TypeKind {
    Enum,
    Function,
    Module,
    Const,
    Static,
    Struct,
    Union,
    Trait,
    Typedef,
    Foreign,
    Macro,
    Attr,
    Derive,
    TraitAlias,
}

crate trait GetDefId {
    fn def_id(&self) -> Option<DefId>;
}

impl<T: GetDefId> GetDefId for Option<T> {
    fn def_id(&self) -> Option<DefId> {
        self.as_ref().and_then(|d| d.def_id())
    }
}

impl Type {
    crate fn primitive_type(&self) -> Option<PrimitiveType> {
        match *self {
            Primitive(p) | BorrowedRef { type_: box Primitive(p), .. } => Some(p),
            Slice(..) | BorrowedRef { type_: box Slice(..), .. } => Some(PrimitiveType::Slice),
            Array(..) | BorrowedRef { type_: box Array(..), .. } => Some(PrimitiveType::Array),
            Tuple(ref tys) => {
                if tys.is_empty() {
                    Some(PrimitiveType::Unit)
                } else {
                    Some(PrimitiveType::Tuple)
                }
            }
            RawPointer(..) => Some(PrimitiveType::RawPointer),
            BorrowedRef { type_: box Generic(..), .. } => Some(PrimitiveType::Reference),
            BareFunction(..) => Some(PrimitiveType::Fn),
            Never => Some(PrimitiveType::Never),
            _ => None,
        }
    }

    crate fn is_generic(&self) -> bool {
        match *self {
            ResolvedPath { is_generic, .. } => is_generic,
            _ => false,
        }
    }

    crate fn is_self_type(&self) -> bool {
        match *self {
            Generic(ref name) => name == "Self",
            _ => false,
        }
    }

    crate fn generics(&self) -> Option<Vec<Type>> {
        match *self {
            ResolvedPath { ref path, .. } => path.segments.last().and_then(|seg| {
                if let GenericArgs::AngleBracketed { ref args, .. } = seg.args {
                    Some(
                        args.iter()
                            .filter_map(|arg| match arg {
                                GenericArg::Type(ty) => Some(ty.clone()),
                                _ => None,
                            })
                            .collect(),
                    )
                } else {
                    None
                }
            }),
            _ => None,
        }
    }

    crate fn bindings(&self) -> Option<&[TypeBinding]> {
        match *self {
            ResolvedPath { ref path, .. } => path.segments.last().and_then(|seg| {
                if let GenericArgs::AngleBracketed { ref bindings, .. } = seg.args {
                    Some(&**bindings)
                } else {
                    None
                }
            }),
            _ => None,
        }
    }

    crate fn is_full_generic(&self) -> bool {
        match *self {
            Type::Generic(_) => true,
            _ => false,
        }
    }

    crate fn projection(&self) -> Option<(&Type, DefId, &str)> {
        let (self_, trait_, name) = match self {
            QPath { ref self_type, ref trait_, ref name } => (self_type, trait_, name),
            _ => return None,
        };
        let trait_did = match **trait_ {
            ResolvedPath { did, .. } => did,
            _ => return None,
        };
        Some((&self_, trait_did, name))
    }
}

impl GetDefId for Type {
    fn def_id(&self) -> Option<DefId> {
        match *self {
            ResolvedPath { did, .. } => Some(did),
            Primitive(p) => cache().primitive_locations.get(&p).cloned(),
            BorrowedRef { type_: box Generic(..), .. } => {
                Primitive(PrimitiveType::Reference).def_id()
            }
            BorrowedRef { ref type_, .. } => type_.def_id(),
            Tuple(ref tys) => {
                if tys.is_empty() {
                    Primitive(PrimitiveType::Unit).def_id()
                } else {
                    Primitive(PrimitiveType::Tuple).def_id()
                }
            }
            BareFunction(..) => Primitive(PrimitiveType::Fn).def_id(),
            Never => Primitive(PrimitiveType::Never).def_id(),
            Slice(..) => Primitive(PrimitiveType::Slice).def_id(),
            Array(..) => Primitive(PrimitiveType::Array).def_id(),
            RawPointer(..) => Primitive(PrimitiveType::RawPointer).def_id(),
            QPath { ref self_type, .. } => self_type.def_id(),
            _ => None,
        }
    }
}

impl PrimitiveType {
    crate fn from_hir(prim: hir::PrimTy) -> PrimitiveType {
        match prim {
            hir::PrimTy::Int(IntTy::Isize) => PrimitiveType::Isize,
            hir::PrimTy::Int(IntTy::I8) => PrimitiveType::I8,
            hir::PrimTy::Int(IntTy::I16) => PrimitiveType::I16,
            hir::PrimTy::Int(IntTy::I32) => PrimitiveType::I32,
            hir::PrimTy::Int(IntTy::I64) => PrimitiveType::I64,
            hir::PrimTy::Int(IntTy::I128) => PrimitiveType::I128,
            hir::PrimTy::Uint(UintTy::Usize) => PrimitiveType::Usize,
            hir::PrimTy::Uint(UintTy::U8) => PrimitiveType::U8,
            hir::PrimTy::Uint(UintTy::U16) => PrimitiveType::U16,
            hir::PrimTy::Uint(UintTy::U32) => PrimitiveType::U32,
            hir::PrimTy::Uint(UintTy::U64) => PrimitiveType::U64,
            hir::PrimTy::Uint(UintTy::U128) => PrimitiveType::U128,
            hir::PrimTy::Float(FloatTy::F32) => PrimitiveType::F32,
            hir::PrimTy::Float(FloatTy::F64) => PrimitiveType::F64,
            hir::PrimTy::Str => PrimitiveType::Str,
            hir::PrimTy::Bool => PrimitiveType::Bool,
            hir::PrimTy::Char => PrimitiveType::Char,
        }
    }

    crate fn from_symbol(s: Symbol) -> Option<PrimitiveType> {
        match s {
            sym::isize => Some(PrimitiveType::Isize),
            sym::i8 => Some(PrimitiveType::I8),
            sym::i16 => Some(PrimitiveType::I16),
            sym::i32 => Some(PrimitiveType::I32),
            sym::i64 => Some(PrimitiveType::I64),
            sym::i128 => Some(PrimitiveType::I128),
            sym::usize => Some(PrimitiveType::Usize),
            sym::u8 => Some(PrimitiveType::U8),
            sym::u16 => Some(PrimitiveType::U16),
            sym::u32 => Some(PrimitiveType::U32),
            sym::u64 => Some(PrimitiveType::U64),
            sym::u128 => Some(PrimitiveType::U128),
            sym::bool => Some(PrimitiveType::Bool),
            sym::char => Some(PrimitiveType::Char),
            sym::str => Some(PrimitiveType::Str),
            sym::f32 => Some(PrimitiveType::F32),
            sym::f64 => Some(PrimitiveType::F64),
            sym::array => Some(PrimitiveType::Array),
            sym::slice => Some(PrimitiveType::Slice),
            sym::tuple => Some(PrimitiveType::Tuple),
            sym::unit => Some(PrimitiveType::Unit),
            sym::pointer => Some(PrimitiveType::RawPointer),
            sym::reference => Some(PrimitiveType::Reference),
            kw::Fn => Some(PrimitiveType::Fn),
            sym::never => Some(PrimitiveType::Never),
            _ => None,
        }
    }

    crate fn as_str(&self) -> &'static str {
        use self::PrimitiveType::*;
        match *self {
            Isize => "isize",
            I8 => "i8",
            I16 => "i16",
            I32 => "i32",
            I64 => "i64",
            I128 => "i128",
            Usize => "usize",
            U8 => "u8",
            U16 => "u16",
            U32 => "u32",
            U64 => "u64",
            U128 => "u128",
            F32 => "f32",
            F64 => "f64",
            Str => "str",
            Bool => "bool",
            Char => "char",
            Array => "array",
            Slice => "slice",
            Tuple => "tuple",
            Unit => "unit",
            RawPointer => "pointer",
            Reference => "reference",
            Fn => "fn",
            Never => "never",
        }
    }

    crate fn impls(&self, tcx: TyCtxt<'_>) -> &'static SmallVec<[DefId; 4]> {
        Self::all_impls(tcx).get(self).expect("missing impl for primitive type")
    }

    crate fn all_impls(tcx: TyCtxt<'_>) -> &'static FxHashMap<PrimitiveType, SmallVec<[DefId; 4]>> {
        static CELL: OnceCell<FxHashMap<PrimitiveType, SmallVec<[DefId; 4]>>> = OnceCell::new();

        CELL.get_or_init(move || {
            use self::PrimitiveType::*;

            /// A macro to create a FxHashMap.
            ///
            /// Example:
            ///
            /// ```
            /// let letters = map!{"a" => "b", "c" => "d"};
            /// ```
            ///
            /// Trailing commas are allowed.
            /// Commas between elements are required (even if the expression is a block).
            macro_rules! map {
                ($( $key: expr => $val: expr ),* $(,)*) => {{
                    let mut map = ::rustc_data_structures::fx::FxHashMap::default();
                    $( map.insert($key, $val); )*
                    map
                }}
            }

            let single = |a: Option<DefId>| a.into_iter().collect();
            let both = |a: Option<DefId>, b: Option<DefId>| -> SmallVec<_> {
                a.into_iter().chain(b).collect()
            };

            let lang_items = tcx.lang_items();
            map! {
                Isize => single(lang_items.isize_impl()),
                I8 => single(lang_items.i8_impl()),
                I16 => single(lang_items.i16_impl()),
                I32 => single(lang_items.i32_impl()),
                I64 => single(lang_items.i64_impl()),
                I128 => single(lang_items.i128_impl()),
                Usize => single(lang_items.usize_impl()),
                U8 => single(lang_items.u8_impl()),
                U16 => single(lang_items.u16_impl()),
                U32 => single(lang_items.u32_impl()),
                U64 => single(lang_items.u64_impl()),
                U128 => single(lang_items.u128_impl()),
                F32 => both(lang_items.f32_impl(), lang_items.f32_runtime_impl()),
                F64 => both(lang_items.f64_impl(), lang_items.f64_runtime_impl()),
                Char => single(lang_items.char_impl()),
                Bool => single(lang_items.bool_impl()),
                Str => both(lang_items.str_impl(), lang_items.str_alloc_impl()),
                Slice => {
                    lang_items
                        .slice_impl()
                        .into_iter()
                        .chain(lang_items.slice_u8_impl())
                        .chain(lang_items.slice_alloc_impl())
                        .chain(lang_items.slice_u8_alloc_impl())
                        .collect()
                },
                Array => single(lang_items.array_impl()),
                Tuple => smallvec![],
                Unit => smallvec![],
                RawPointer => {
                    lang_items
                        .const_ptr_impl()
                        .into_iter()
                        .chain(lang_items.mut_ptr_impl())
                        .chain(lang_items.const_slice_ptr_impl())
                        .chain(lang_items.mut_slice_ptr_impl())
                        .collect()
                },
                Reference => smallvec![],
                Fn => smallvec![],
                Never => smallvec![],
            }
        })
    }

    crate fn to_url_str(&self) -> &'static str {
        self.as_str()
    }
}

impl From<ast::IntTy> for PrimitiveType {
    fn from(int_ty: ast::IntTy) -> PrimitiveType {
        match int_ty {
            ast::IntTy::Isize => PrimitiveType::Isize,
            ast::IntTy::I8 => PrimitiveType::I8,
            ast::IntTy::I16 => PrimitiveType::I16,
            ast::IntTy::I32 => PrimitiveType::I32,
            ast::IntTy::I64 => PrimitiveType::I64,
            ast::IntTy::I128 => PrimitiveType::I128,
        }
    }
}

impl From<ast::UintTy> for PrimitiveType {
    fn from(uint_ty: ast::UintTy) -> PrimitiveType {
        match uint_ty {
            ast::UintTy::Usize => PrimitiveType::Usize,
            ast::UintTy::U8 => PrimitiveType::U8,
            ast::UintTy::U16 => PrimitiveType::U16,
            ast::UintTy::U32 => PrimitiveType::U32,
            ast::UintTy::U64 => PrimitiveType::U64,
            ast::UintTy::U128 => PrimitiveType::U128,
        }
    }
}

impl From<ast::FloatTy> for PrimitiveType {
    fn from(float_ty: ast::FloatTy) -> PrimitiveType {
        match float_ty {
            ast::FloatTy::F32 => PrimitiveType::F32,
            ast::FloatTy::F64 => PrimitiveType::F64,
        }
    }
}

impl From<hir::PrimTy> for PrimitiveType {
    fn from(prim_ty: hir::PrimTy) -> PrimitiveType {
        match prim_ty {
            hir::PrimTy::Int(int_ty) => int_ty.into(),
            hir::PrimTy::Uint(uint_ty) => uint_ty.into(),
            hir::PrimTy::Float(float_ty) => float_ty.into(),
            hir::PrimTy::Str => PrimitiveType::Str,
            hir::PrimTy::Bool => PrimitiveType::Bool,
            hir::PrimTy::Char => PrimitiveType::Char,
        }
    }
}

#[derive(Clone, Debug)]
crate enum Visibility {
    Public,
    Inherited,
    Restricted(DefId, rustc_hir::definitions::DefPath),
}

impl Visibility {
    crate fn is_public(&self) -> bool {
        matches!(self, Visibility::Public)
    }
}

#[derive(Clone, Debug)]
crate struct Struct {
    crate struct_type: doctree::StructType,
    crate generics: Generics,
    crate fields: Vec<Item>,
    crate fields_stripped: bool,
}

#[derive(Clone, Debug)]
crate struct Union {
    crate struct_type: doctree::StructType,
    crate generics: Generics,
    crate fields: Vec<Item>,
    crate fields_stripped: bool,
}

/// This is a more limited form of the standard Struct, different in that
/// it lacks the things most items have (name, id, parameterization). Found
/// only as a variant in an enum.
#[derive(Clone, Debug)]
crate struct VariantStruct {
    crate struct_type: doctree::StructType,
    crate fields: Vec<Item>,
    crate fields_stripped: bool,
}

#[derive(Clone, Debug)]
crate struct Enum {
    crate variants: IndexVec<VariantIdx, Item>,
    crate generics: Generics,
    crate variants_stripped: bool,
}

#[derive(Clone, Debug)]
crate struct Variant {
    crate kind: VariantKind,
}

#[derive(Clone, Debug)]
crate enum VariantKind {
    CLike,
    Tuple(Vec<Type>),
    Struct(VariantStruct),
}

#[derive(Clone, Debug)]
crate struct Span {
    crate filename: FileName,
    crate cnum: CrateNum,
    crate loline: usize,
    crate locol: usize,
    crate hiline: usize,
    crate hicol: usize,
    crate original: rustc_span::Span,
}

impl Span {
    crate fn empty() -> Span {
        Span {
            filename: FileName::Anon(0),
            cnum: LOCAL_CRATE,
            loline: 0,
            locol: 0,
            hiline: 0,
            hicol: 0,
            original: rustc_span::DUMMY_SP,
        }
    }

    crate fn span(&self) -> rustc_span::Span {
        self.original
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Hash)]
crate struct Path {
    crate global: bool,
    crate res: Res,
    crate segments: Vec<PathSegment>,
}

impl Path {
    crate fn last_name(&self) -> &str {
        self.segments.last().expect("segments were empty").name.as_str()
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Hash)]
crate enum GenericArg {
    Lifetime(Lifetime),
    Type(Type),
    Const(Constant),
}

#[derive(Clone, PartialEq, Eq, Debug, Hash)]
crate enum GenericArgs {
    AngleBracketed { args: Vec<GenericArg>, bindings: Vec<TypeBinding> },
    Parenthesized { inputs: Vec<Type>, output: Option<Type> },
}

#[derive(Clone, PartialEq, Eq, Debug, Hash)]
crate struct PathSegment {
    crate name: String,
    crate args: GenericArgs,
}

#[derive(Clone, Debug)]
crate struct Typedef {
    crate type_: Type,
    crate generics: Generics,
    // Type of target item.
    crate item_type: Option<Type>,
}

impl GetDefId for Typedef {
    fn def_id(&self) -> Option<DefId> {
        self.type_.def_id()
    }
}

#[derive(Clone, Debug)]
crate struct OpaqueTy {
    crate bounds: Vec<GenericBound>,
    crate generics: Generics,
}

#[derive(Clone, PartialEq, Eq, Debug, Hash)]
crate struct BareFunctionDecl {
    crate unsafety: hir::Unsafety,
    crate generic_params: Vec<GenericParamDef>,
    crate decl: FnDecl,
    crate abi: Abi,
}

#[derive(Clone, Debug)]
crate struct Static {
    crate type_: Type,
    crate mutability: Mutability,
    /// It's useful to have the value of a static documented, but I have no
    /// desire to represent expressions (that'd basically be all of the AST,
    /// which is huge!). So, have a string.
    crate expr: String,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
crate struct Constant {
    crate type_: Type,
    crate expr: String,
    crate value: Option<String>,
    crate is_literal: bool,
}

#[derive(Clone, PartialEq, Debug)]
crate enum ImplPolarity {
    Positive,
    Negative,
}

#[derive(Clone, Debug)]
crate struct Impl {
    crate unsafety: hir::Unsafety,
    crate generics: Generics,
    crate provided_trait_methods: FxHashSet<String>,
    crate trait_: Option<Type>,
    crate for_: Type,
    crate items: Vec<Item>,
    crate polarity: Option<ImplPolarity>,
    crate synthetic: bool,
    crate blanket_impl: Option<Type>,
}

#[derive(Clone, Debug)]
crate struct Import {
    crate kind: ImportKind,
    crate source: ImportSource,
    crate should_be_displayed: bool,
}

impl Import {
    crate fn new_simple(name: String, source: ImportSource, should_be_displayed: bool) -> Self {
        Self { kind: ImportKind::Simple(name), source, should_be_displayed }
    }

    crate fn new_glob(source: ImportSource, should_be_displayed: bool) -> Self {
        Self { kind: ImportKind::Glob, source, should_be_displayed }
    }
}

#[derive(Clone, Debug)]
crate enum ImportKind {
    // use source as str;
    Simple(String),
    // use source::*;
    Glob,
}

#[derive(Clone, Debug)]
crate struct ImportSource {
    crate path: Path,
    crate did: Option<DefId>,
}

#[derive(Clone, Debug)]
crate struct Macro {
    crate source: String,
    crate imported_from: Option<String>,
}

#[derive(Clone, Debug)]
crate struct ProcMacro {
    crate kind: MacroKind,
    crate helpers: Vec<String>,
}

#[derive(Clone, Debug)]
crate struct Deprecation {
    crate since: Option<String>,
    crate note: Option<String>,
    crate is_since_rustc_version: bool,
}

/// An type binding on an associated type (e.g., `A = Bar` in `Foo<A = Bar>` or
/// `A: Send + Sync` in `Foo<A: Send + Sync>`).
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
crate struct TypeBinding {
    crate name: String,
    crate kind: TypeBindingKind,
}

#[derive(Clone, PartialEq, Eq, Debug, Hash)]
crate enum TypeBindingKind {
    Equality { ty: Type },
    Constraint { bounds: Vec<GenericBound> },
}

impl TypeBinding {
    crate fn ty(&self) -> &Type {
        match self.kind {
            TypeBindingKind::Equality { ref ty } => ty,
            _ => panic!("expected equality type binding for parenthesized generic args"),
        }
    }
}
