// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! This module contains the "cleaned" pieces of the AST, and the functions
//! that clean them.

pub use self::ImplMethod::*;
pub use self::Type::*;
pub use self::PrimitiveType::*;
pub use self::TypeKind::*;
pub use self::StructField::*;
pub use self::VariantKind::*;
pub use self::Mutability::*;
pub use self::ViewItemInner::*;
pub use self::ViewPath::*;
pub use self::ItemEnum::*;
pub use self::Attribute::*;
pub use self::TyParamBound::*;
pub use self::SelfTy::*;
pub use self::FunctionRetTy::*;
pub use self::TraitMethod::*;

use syntax;
use syntax::ast;
use syntax::ast_util;
use syntax::ast_util::PostExpansionMethod;
use syntax::attr;
use syntax::attr::{AttributeMethods, AttrMetaMethods};
use syntax::codemap::{DUMMY_SP, Pos, Spanned};
use syntax::parse::token::InternedString;
use syntax::parse::token;
use syntax::ptr::P;

use rustc_trans::back::link;
use rustc::metadata::cstore;
use rustc::metadata::csearch;
use rustc::metadata::decoder;
use rustc::middle::def;
use rustc::middle::subst;
use rustc::middle::subst::VecPerParamSpace;
use rustc::middle::ty;
use rustc::middle::stability;
use rustc::session::config;

use std::rc::Rc;
use std::u32;
use std::str::Str as StrTrait; // Conflicts with Str variant
use std::char::Char as CharTrait; // Conflicts with Char variant
use std::path::Path as FsPath; // Conflicts with Path struct

use core::DocContext;
use doctree;
use visit_ast;

/// A stable identifier to the particular version of JSON output.
/// Increment this when the `Crate` and related structures change.
pub static SCHEMA_VERSION: &'static str = "0.8.3";

mod inline;

// extract the stability index for a node from tcx, if possible
fn get_stability(cx: &DocContext, def_id: ast::DefId) -> Option<Stability> {
    cx.tcx_opt().and_then(|tcx| stability::lookup(tcx, def_id)).clean(cx)
}

pub trait Clean<T> {
    fn clean(&self, cx: &DocContext) -> T;
}

impl<T: Clean<U>, U> Clean<Vec<U>> for Vec<T> {
    fn clean(&self, cx: &DocContext) -> Vec<U> {
        self.iter().map(|x| x.clean(cx)).collect()
    }
}

impl<T: Clean<U>, U> Clean<VecPerParamSpace<U>> for VecPerParamSpace<T> {
    fn clean(&self, cx: &DocContext) -> VecPerParamSpace<U> {
        self.map(|x| x.clean(cx))
    }
}

impl<T: Clean<U>, U> Clean<U> for P<T> {
    fn clean(&self, cx: &DocContext) -> U {
        (**self).clean(cx)
    }
}

impl<T: Clean<U>, U> Clean<U> for Rc<T> {
    fn clean(&self, cx: &DocContext) -> U {
        (**self).clean(cx)
    }
}

impl<T: Clean<U>, U> Clean<Option<U>> for Option<T> {
    fn clean(&self, cx: &DocContext) -> Option<U> {
        match self {
            &None => None,
            &Some(ref v) => Some(v.clean(cx))
        }
    }
}

impl<T: Clean<U>, U> Clean<Vec<U>> for syntax::owned_slice::OwnedSlice<T> {
    fn clean(&self, cx: &DocContext) -> Vec<U> {
        self.iter().map(|x| x.clean(cx)).collect()
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub struct Crate {
    pub name: String,
    pub src: FsPath,
    pub module: Option<Item>,
    pub externs: Vec<(ast::CrateNum, ExternalCrate)>,
    pub primitives: Vec<PrimitiveType>,
}

impl<'a, 'tcx> Clean<Crate> for visit_ast::RustdocVisitor<'a, 'tcx> {
    fn clean(&self, cx: &DocContext) -> Crate {
        let mut externs = Vec::new();
        cx.sess().cstore.iter_crate_data(|n, meta| {
            externs.push((n, meta.clean(cx)));
        });
        externs.sort_by(|&(a, _), &(b, _)| a.cmp(&b));

        // Figure out the name of this crate
        let input = config::Input::File(cx.src.clone());
        let name = link::find_crate_name(None, self.attrs.as_slice(), &input);

        // Clean the crate, translating the entire libsyntax AST to one that is
        // understood by rustdoc.
        let mut module = self.module.clean(cx);

        // Collect all inner modules which are tagged as implementations of
        // primitives.
        //
        // Note that this loop only searches the top-level items of the crate,
        // and this is intentional. If we were to search the entire crate for an
        // item tagged with `#[doc(primitive)]` then we we would also have to
        // search the entirety of external modules for items tagged
        // `#[doc(primitive)]`, which is a pretty inefficient process (decoding
        // all that metadata unconditionally).
        //
        // In order to keep the metadata load under control, the
        // `#[doc(primitive)]` feature is explicitly designed to only allow the
        // primitive tags to show up as the top level items in a crate.
        //
        // Also note that this does not attempt to deal with modules tagged
        // duplicately for the same primitive. This is handled later on when
        // rendering by delegating everything to a hash map.
        let mut primitives = Vec::new();
        {
            let m = match module.inner {
                ModuleItem(ref mut m) => m,
                _ => unreachable!(),
            };
            let mut tmp = Vec::new();
            for child in m.items.iter_mut() {
                let inner = match child.inner {
                    ModuleItem(ref mut m) => m,
                    _ => continue,
                };
                let prim = match PrimitiveType::find(child.attrs.as_slice()) {
                    Some(prim) => prim,
                    None => continue,
                };
                primitives.push(prim);
                let mut i = Item {
                    source: Span::empty(),
                    name: Some(prim.to_url_str().to_string()),
                    attrs: Vec::new(),
                    visibility: None,
                    stability: None,
                    def_id: ast_util::local_def(prim.to_node_id()),
                    inner: PrimitiveItem(prim),
                };
                // Push one copy to get indexed for the whole crate, and push a
                // another copy in the proper location which will actually get
                // documented. The first copy will also serve as a redirect to
                // the other copy.
                tmp.push(i.clone());
                i.visibility = Some(ast::Public);
                i.attrs = child.attrs.clone();
                inner.items.push(i);

            }
            m.items.extend(tmp.into_iter());
        }

        Crate {
            name: name.to_string(),
            src: cx.src.clone(),
            module: Some(module),
            externs: externs,
            primitives: primitives,
        }
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub struct ExternalCrate {
    pub name: String,
    pub attrs: Vec<Attribute>,
    pub primitives: Vec<PrimitiveType>,
}

impl Clean<ExternalCrate> for cstore::crate_metadata {
    fn clean(&self, cx: &DocContext) -> ExternalCrate {
        let mut primitives = Vec::new();
        cx.tcx_opt().map(|tcx| {
            csearch::each_top_level_item_of_crate(&tcx.sess.cstore,
                                                  self.cnum,
                                                  |def, _, _| {
                let did = match def {
                    decoder::DlDef(def::DefMod(did)) => did,
                    _ => return
                };
                let attrs = inline::load_attrs(cx, tcx, did);
                PrimitiveType::find(attrs.as_slice()).map(|prim| primitives.push(prim));
            })
        });
        ExternalCrate {
            name: self.name.to_string(),
            attrs: decoder::get_crate_attributes(self.data()).clean(cx),
            primitives: primitives,
        }
    }
}

/// Anything with a source location and set of attributes and, optionally, a
/// name. That is, anything that can be documented. This doesn't correspond
/// directly to the AST's concept of an item; it's a strict superset.
#[deriving(Clone, Encodable, Decodable)]
pub struct Item {
    /// Stringified span
    pub source: Span,
    /// Not everything has a name. E.g., impls
    pub name: Option<String>,
    pub attrs: Vec<Attribute> ,
    pub inner: ItemEnum,
    pub visibility: Option<Visibility>,
    pub def_id: ast::DefId,
    pub stability: Option<Stability>,
}

impl Item {
    /// Finds the `doc` attribute as a List and returns the list of attributes
    /// nested inside.
    pub fn doc_list<'a>(&'a self) -> Option<&'a [Attribute]> {
        for attr in self.attrs.iter() {
            match *attr {
                List(ref x, ref list) if "doc" == *x => {
                    return Some(list.as_slice());
                }
                _ => {}
            }
        }
        return None;
    }

    /// Finds the `doc` attribute as a NameValue and returns the corresponding
    /// value found.
    pub fn doc_value<'a>(&'a self) -> Option<&'a str> {
        for attr in self.attrs.iter() {
            match *attr {
                NameValue(ref x, ref v) if "doc" == *x => {
                    return Some(v.as_slice());
                }
                _ => {}
            }
        }
        return None;
    }

    pub fn is_hidden_from_doc(&self) -> bool {
        match self.doc_list() {
            Some(ref l) => {
                for innerattr in l.iter() {
                    match *innerattr {
                        Word(ref s) if "hidden" == *s => {
                            return true
                        }
                        _ => (),
                    }
                }
            },
            None => ()
        }
        return false;
    }

    pub fn is_mod(&self) -> bool {
        match self.inner { ModuleItem(..) => true, _ => false }
    }
    pub fn is_trait(&self) -> bool {
        match self.inner { TraitItem(..) => true, _ => false }
    }
    pub fn is_struct(&self) -> bool {
        match self.inner { StructItem(..) => true, _ => false }
    }
    pub fn is_enum(&self) -> bool {
        match self.inner { EnumItem(..) => true, _ => false }
    }
    pub fn is_fn(&self) -> bool {
        match self.inner { FunctionItem(..) => true, _ => false }
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub enum ItemEnum {
    StructItem(Struct),
    EnumItem(Enum),
    FunctionItem(Function),
    ModuleItem(Module),
    TypedefItem(Typedef),
    StaticItem(Static),
    ConstantItem(Constant),
    TraitItem(Trait),
    ImplItem(Impl),
    /// `use` and `extern crate`
    ViewItemItem(ViewItem),
    /// A method signature only. Used for required methods in traits (ie,
    /// non-default-methods).
    TyMethodItem(TyMethod),
    /// A method with a body.
    MethodItem(Method),
    StructFieldItem(StructField),
    VariantItem(Variant),
    /// `fn`s from an extern block
    ForeignFunctionItem(Function),
    /// `static`s from an extern block
    ForeignStaticItem(Static),
    MacroItem(Macro),
    PrimitiveItem(PrimitiveType),
    AssociatedTypeItem(TyParam),
}

#[deriving(Clone, Encodable, Decodable)]
pub struct Module {
    pub items: Vec<Item>,
    pub is_crate: bool,
}

impl Clean<Item> for doctree::Module {
    fn clean(&self, cx: &DocContext) -> Item {
        let name = if self.name.is_some() {
            self.name.unwrap().clean(cx)
        } else {
            "".to_string()
        };
        let mut foreigns = Vec::new();
        for subforeigns in self.foreigns.clean(cx).into_iter() {
            for foreign in subforeigns.into_iter() {
                foreigns.push(foreign)
            }
        }
        let items: Vec<Vec<Item> > = vec!(
            self.structs.clean(cx),
            self.enums.clean(cx),
            self.fns.clean(cx),
            foreigns,
            self.mods.clean(cx),
            self.typedefs.clean(cx),
            self.statics.clean(cx),
            self.constants.clean(cx),
            self.traits.clean(cx),
            self.impls.clean(cx),
            self.view_items.clean(cx).into_iter()
                           .flat_map(|s| s.into_iter()).collect(),
            self.macros.clean(cx),
        );

        // determine if we should display the inner contents or
        // the outer `mod` item for the source code.
        let whence = {
            let cm = cx.sess().codemap();
            let outer = cm.lookup_char_pos(self.where_outer.lo);
            let inner = cm.lookup_char_pos(self.where_inner.lo);
            if outer.file.start_pos == inner.file.start_pos {
                // mod foo { ... }
                self.where_outer
            } else {
                // mod foo; (and a separate FileMap for the contents)
                self.where_inner
            }
        };

        Item {
            name: Some(name),
            attrs: self.attrs.clean(cx),
            source: whence.clean(cx),
            visibility: self.vis.clean(cx),
            stability: self.stab.clean(cx),
            def_id: ast_util::local_def(self.id),
            inner: ModuleItem(Module {
               is_crate: self.is_crate,
               items: items.iter()
                           .flat_map(|x| x.iter().map(|x| (*x).clone()))
                           .collect(),
            })
        }
    }
}

#[deriving(Clone, Encodable, Decodable, PartialEq)]
pub enum Attribute {
    Word(String),
    List(String, Vec<Attribute> ),
    NameValue(String, String)
}

impl Clean<Attribute> for ast::MetaItem {
    fn clean(&self, cx: &DocContext) -> Attribute {
        match self.node {
            ast::MetaWord(ref s) => Word(s.get().to_string()),
            ast::MetaList(ref s, ref l) => {
                List(s.get().to_string(), l.clean(cx))
            }
            ast::MetaNameValue(ref s, ref v) => {
                NameValue(s.get().to_string(), lit_to_string(v))
            }
        }
    }
}

impl Clean<Attribute> for ast::Attribute {
    fn clean(&self, cx: &DocContext) -> Attribute {
        self.with_desugared_doc(|a| a.node.value.clean(cx))
    }
}

// This is a rough approximation that gets us what we want.
impl attr::AttrMetaMethods for Attribute {
    fn name(&self) -> InternedString {
        match *self {
            Word(ref n) | List(ref n, _) | NameValue(ref n, _) => {
                token::intern_and_get_ident(n.as_slice())
            }
        }
    }

    fn value_str(&self) -> Option<InternedString> {
        match *self {
            NameValue(_, ref v) => {
                Some(token::intern_and_get_ident(v.as_slice()))
            }
            _ => None,
        }
    }
    fn meta_item_list<'a>(&'a self) -> Option<&'a [P<ast::MetaItem>]> { None }
}
impl<'a> attr::AttrMetaMethods for &'a Attribute {
    fn name(&self) -> InternedString { (**self).name() }
    fn value_str(&self) -> Option<InternedString> { (**self).value_str() }
    fn meta_item_list<'a>(&'a self) -> Option<&'a [P<ast::MetaItem>]> { None }
}

#[deriving(Clone, Encodable, Decodable, PartialEq)]
pub struct TyParam {
    pub name: String,
    pub did: ast::DefId,
    pub bounds: Vec<TyParamBound>,
    pub default: Option<Type>,
    /// An optional default bound on the parameter which is unbound, like `Sized?`
    pub default_unbound: Option<Type>
}

impl Clean<TyParam> for ast::TyParam {
    fn clean(&self, cx: &DocContext) -> TyParam {
        TyParam {
            name: self.ident.clean(cx),
            did: ast::DefId { krate: ast::LOCAL_CRATE, node: self.id },
            bounds: self.bounds.clean(cx),
            default: self.default.clean(cx),
            default_unbound: self.unbound.clean(cx)
        }
    }
}

impl<'tcx> Clean<TyParam> for ty::TypeParameterDef<'tcx> {
    fn clean(&self, cx: &DocContext) -> TyParam {
        cx.external_typarams.borrow_mut().as_mut().unwrap()
          .insert(self.def_id, self.name.clean(cx));
        let (bounds, default_unbound) = self.bounds.clean(cx);
        TyParam {
            name: self.name.clean(cx),
            did: self.def_id,
            bounds: bounds,
            default: self.default.clean(cx),
            default_unbound: default_unbound
        }
    }
}

#[deriving(Clone, Encodable, Decodable, PartialEq)]
pub enum TyParamBound {
    RegionBound(Lifetime),
    TraitBound(Type)
}

impl Clean<TyParamBound> for ast::TyParamBound {
    fn clean(&self, cx: &DocContext) -> TyParamBound {
        match *self {
            ast::RegionTyParamBound(lt) => RegionBound(lt.clean(cx)),
            ast::TraitTyParamBound(ref t) => TraitBound(t.clean(cx)),
        }
    }
}

impl Clean<Vec<TyParamBound>> for ty::ExistentialBounds {
    fn clean(&self, cx: &DocContext) -> Vec<TyParamBound> {
        let mut vec = vec![];
        self.region_bound.clean(cx).map(|b| vec.push(RegionBound(b)));
        for bb in self.builtin_bounds.iter() {
            vec.push(bb.clean(cx));
        }
        vec
    }
}

fn external_path(cx: &DocContext, name: &str, substs: &subst::Substs) -> Path {
    let lifetimes = substs.regions().get_slice(subst::TypeSpace)
                    .iter()
                    .filter_map(|v| v.clean(cx))
                    .collect();
    let types = substs.types.get_slice(subst::TypeSpace).to_vec();
    let types = types.clean(cx);
    Path {
        global: false,
        segments: vec![PathSegment {
            name: name.to_string(),
            lifetimes: lifetimes,
            types: types,
        }],
    }
}

impl Clean<TyParamBound> for ty::BuiltinBound {
    fn clean(&self, cx: &DocContext) -> TyParamBound {
        let tcx = match cx.tcx_opt() {
            Some(tcx) => tcx,
            None => return RegionBound(Lifetime::statik())
        };
        let empty = subst::Substs::empty();
        let (did, path) = match *self {
            ty::BoundSend =>
                (tcx.lang_items.send_trait().unwrap(),
                 external_path(cx, "Send", &empty)),
            ty::BoundSized =>
                (tcx.lang_items.sized_trait().unwrap(),
                 external_path(cx, "Sized", &empty)),
            ty::BoundCopy =>
                (tcx.lang_items.copy_trait().unwrap(),
                 external_path(cx, "Copy", &empty)),
            ty::BoundSync =>
                (tcx.lang_items.sync_trait().unwrap(),
                 external_path(cx, "Sync", &empty)),
        };
        let fqn = csearch::get_item_path(tcx, did);
        let fqn = fqn.into_iter().map(|i| i.to_string()).collect();
        cx.external_paths.borrow_mut().as_mut().unwrap().insert(did,
                                                                (fqn, TypeTrait));
        TraitBound(ResolvedPath {
            path: path,
            typarams: None,
            did: did,
        })
    }
}

impl<'tcx> Clean<TyParamBound> for ty::TraitRef<'tcx> {
    fn clean(&self, cx: &DocContext) -> TyParamBound {
        let tcx = match cx.tcx_opt() {
            Some(tcx) => tcx,
            None => return RegionBound(Lifetime::statik())
        };
        let fqn = csearch::get_item_path(tcx, self.def_id);
        let fqn = fqn.into_iter().map(|i| i.to_string())
                     .collect::<Vec<String>>();
        let path = external_path(cx, fqn.last().unwrap().as_slice(),
                                 &self.substs);
        cx.external_paths.borrow_mut().as_mut().unwrap().insert(self.def_id,
                                                            (fqn, TypeTrait));
        TraitBound(ResolvedPath {
            path: path,
            typarams: None,
            did: self.def_id,
        })
    }
}

// Returns (bounds, default_unbound)
impl<'tcx> Clean<(Vec<TyParamBound>, Option<Type>)> for ty::ParamBounds<'tcx> {
    fn clean(&self, cx: &DocContext) -> (Vec<TyParamBound>, Option<Type>) {
        let mut v = Vec::new();
        let mut has_sized_bound = false;
        for b in self.builtin_bounds.iter() {
            if b != ty::BoundSized {
                v.push(b.clean(cx));
            } else {
                has_sized_bound = true;
            }
        }
        for t in self.trait_bounds.iter() {
            v.push(t.clean(cx));
        }
        for r in self.region_bounds.iter().filter_map(|r| r.clean(cx)) {
            v.push(RegionBound(r));
        }
        if has_sized_bound {
            (v, None)
        } else {
            let ty = match ty::BoundSized.clean(cx) {
                TraitBound(ty) => ty,
                _ => unreachable!()
            };
            (v, Some(ty))
        }
    }
}

impl<'tcx> Clean<Option<Vec<TyParamBound>>> for subst::Substs<'tcx> {
    fn clean(&self, cx: &DocContext) -> Option<Vec<TyParamBound>> {
        let mut v = Vec::new();
        v.extend(self.regions().iter().filter_map(|r| r.clean(cx)).map(RegionBound));
        v.extend(self.types.iter().map(|t| TraitBound(t.clean(cx))));
        if v.len() > 0 {Some(v)} else {None}
    }
}

#[deriving(Clone, Encodable, Decodable, PartialEq)]
pub struct Lifetime(String);

impl Lifetime {
    pub fn get_ref<'a>(&'a self) -> &'a str {
        let Lifetime(ref s) = *self;
        let s: &'a str = s.as_slice();
        return s;
    }

    pub fn statik() -> Lifetime {
        Lifetime("'static".to_string())
    }
}

impl Clean<Lifetime> for ast::Lifetime {
    fn clean(&self, _: &DocContext) -> Lifetime {
        Lifetime(token::get_name(self.name).get().to_string())
    }
}

impl Clean<Lifetime> for ast::LifetimeDef {
    fn clean(&self, _: &DocContext) -> Lifetime {
        Lifetime(token::get_name(self.lifetime.name).get().to_string())
    }
}

impl Clean<Lifetime> for ty::RegionParameterDef {
    fn clean(&self, _: &DocContext) -> Lifetime {
        Lifetime(token::get_name(self.name).get().to_string())
    }
}

impl Clean<Option<Lifetime>> for ty::Region {
    fn clean(&self, cx: &DocContext) -> Option<Lifetime> {
        match *self {
            ty::ReStatic => Some(Lifetime::statik()),
            ty::ReLateBound(_, ty::BrNamed(_, name)) =>
                Some(Lifetime(token::get_name(name).get().to_string())),
            ty::ReEarlyBound(_, _, _, name) => Some(Lifetime(name.clean(cx))),

            ty::ReLateBound(..) |
            ty::ReFree(..) |
            ty::ReScope(..) |
            ty::ReInfer(..) |
            ty::ReEmpty(..) => None
        }
    }
}

#[deriving(Clone, Encodable, Decodable, PartialEq)]
pub struct WherePredicate {
    pub name: String,
    pub bounds: Vec<TyParamBound>
}

impl Clean<WherePredicate> for ast::WherePredicate {
    fn clean(&self, cx: &DocContext) -> WherePredicate {
        WherePredicate {
            name: self.ident.clean(cx),
            bounds: self.bounds.clean(cx)
        }
    }
}

// maybe use a Generic enum and use ~[Generic]?
#[deriving(Clone, Encodable, Decodable, PartialEq)]
pub struct Generics {
    pub lifetimes: Vec<Lifetime>,
    pub type_params: Vec<TyParam>,
    pub where_predicates: Vec<WherePredicate>
}

impl Clean<Generics> for ast::Generics {
    fn clean(&self, cx: &DocContext) -> Generics {
        Generics {
            lifetimes: self.lifetimes.clean(cx),
            type_params: self.ty_params.clean(cx),
            where_predicates: self.where_clause.predicates.clean(cx)
        }
    }
}

impl<'a, 'tcx> Clean<Generics> for (&'a ty::Generics<'tcx>, subst::ParamSpace) {
    fn clean(&self, cx: &DocContext) -> Generics {
        let (me, space) = *self;
        Generics {
            type_params: me.types.get_slice(space).to_vec().clean(cx),
            lifetimes: me.regions.get_slice(space).to_vec().clean(cx),
            where_predicates: vec![]
        }
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub struct Method {
    pub generics: Generics,
    pub self_: SelfTy,
    pub fn_style: ast::FnStyle,
    pub decl: FnDecl,
}

impl Clean<Item> for ast::Method {
    fn clean(&self, cx: &DocContext) -> Item {
        let all_inputs = &self.pe_fn_decl().inputs;
        let inputs = match self.pe_explicit_self().node {
            ast::SelfStatic => all_inputs.as_slice(),
            _ => all_inputs[1..]
        };
        let decl = FnDecl {
            inputs: Arguments {
                values: inputs.iter().map(|x| x.clean(cx)).collect(),
            },
            output: self.pe_fn_decl().output.clean(cx),
            attrs: Vec::new()
        };
        Item {
            name: Some(self.pe_ident().clean(cx)),
            attrs: self.attrs.clean(cx),
            source: self.span.clean(cx),
            def_id: ast_util::local_def(self.id),
            visibility: self.pe_vis().clean(cx),
            stability: get_stability(cx, ast_util::local_def(self.id)),
            inner: MethodItem(Method {
                generics: self.pe_generics().clean(cx),
                self_: self.pe_explicit_self().node.clean(cx),
                fn_style: self.pe_fn_style().clone(),
                decl: decl,
            }),
        }
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub struct TyMethod {
    pub fn_style: ast::FnStyle,
    pub decl: FnDecl,
    pub generics: Generics,
    pub self_: SelfTy,
}

impl Clean<Item> for ast::TypeMethod {
    fn clean(&self, cx: &DocContext) -> Item {
        let inputs = match self.explicit_self.node {
            ast::SelfStatic => self.decl.inputs.as_slice(),
            _ => self.decl.inputs[1..]
        };
        let decl = FnDecl {
            inputs: Arguments {
                values: inputs.iter().map(|x| x.clean(cx)).collect(),
            },
            output: self.decl.output.clean(cx),
            attrs: Vec::new()
        };
        Item {
            name: Some(self.ident.clean(cx)),
            attrs: self.attrs.clean(cx),
            source: self.span.clean(cx),
            def_id: ast_util::local_def(self.id),
            visibility: None,
            stability: get_stability(cx, ast_util::local_def(self.id)),
            inner: TyMethodItem(TyMethod {
                fn_style: self.fn_style.clone(),
                decl: decl,
                self_: self.explicit_self.node.clean(cx),
                generics: self.generics.clean(cx),
            }),
        }
    }
}

#[deriving(Clone, Encodable, Decodable, PartialEq)]
pub enum SelfTy {
    SelfStatic,
    SelfValue,
    SelfBorrowed(Option<Lifetime>, Mutability),
    SelfExplicit(Type),
}

impl Clean<SelfTy> for ast::ExplicitSelf_ {
    fn clean(&self, cx: &DocContext) -> SelfTy {
        match *self {
            ast::SelfStatic => SelfStatic,
            ast::SelfValue(_) => SelfValue,
            ast::SelfRegion(ref lt, ref mt, _) => {
                SelfBorrowed(lt.clean(cx), mt.clean(cx))
            }
            ast::SelfExplicit(ref typ, _) => SelfExplicit(typ.clean(cx)),
        }
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub struct Function {
    pub decl: FnDecl,
    pub generics: Generics,
    pub fn_style: ast::FnStyle,
}

impl Clean<Item> for doctree::Function {
    fn clean(&self, cx: &DocContext) -> Item {
        Item {
            name: Some(self.name.clean(cx)),
            attrs: self.attrs.clean(cx),
            source: self.whence.clean(cx),
            visibility: self.vis.clean(cx),
            stability: self.stab.clean(cx),
            def_id: ast_util::local_def(self.id),
            inner: FunctionItem(Function {
                decl: self.decl.clean(cx),
                generics: self.generics.clean(cx),
                fn_style: self.fn_style,
            }),
        }
    }
}

#[deriving(Clone, Encodable, Decodable, PartialEq)]
pub struct ClosureDecl {
    pub lifetimes: Vec<Lifetime>,
    pub decl: FnDecl,
    pub onceness: ast::Onceness,
    pub fn_style: ast::FnStyle,
    pub bounds: Vec<TyParamBound>,
}

impl Clean<ClosureDecl> for ast::ClosureTy {
    fn clean(&self, cx: &DocContext) -> ClosureDecl {
        ClosureDecl {
            lifetimes: self.lifetimes.clean(cx),
            decl: self.decl.clean(cx),
            onceness: self.onceness,
            fn_style: self.fn_style,
            bounds: self.bounds.clean(cx)
        }
    }
}

#[deriving(Clone, Encodable, Decodable, PartialEq)]
pub struct FnDecl {
    pub inputs: Arguments,
    pub output: FunctionRetTy,
    pub attrs: Vec<Attribute>,
}

#[deriving(Clone, Encodable, Decodable, PartialEq)]
pub struct Arguments {
    pub values: Vec<Argument>,
}

impl Clean<FnDecl> for ast::FnDecl {
    fn clean(&self, cx: &DocContext) -> FnDecl {
        FnDecl {
            inputs: Arguments {
                values: self.inputs.clean(cx),
            },
            output: self.output.clean(cx),
            attrs: Vec::new()
        }
    }
}

impl<'tcx> Clean<Type> for ty::FnOutput<'tcx> {
    fn clean(&self, cx: &DocContext) -> Type {
        match *self {
            ty::FnConverging(ty) => ty.clean(cx),
            ty::FnDiverging => Bottom
        }
    }
}

impl<'a, 'tcx> Clean<FnDecl> for (ast::DefId, &'a ty::FnSig<'tcx>) {
    fn clean(&self, cx: &DocContext) -> FnDecl {
        let (did, sig) = *self;
        let mut names = if did.node != 0 {
            csearch::get_method_arg_names(&cx.tcx().sess.cstore, did).into_iter()
        } else {
            Vec::new().into_iter()
        }.peekable();
        if names.peek().map(|s| s.as_slice()) == Some("self") {
            let _ = names.next();
        }
        FnDecl {
            output: Return(sig.output.clean(cx)),
            attrs: Vec::new(),
            inputs: Arguments {
                values: sig.inputs.iter().map(|t| {
                    Argument {
                        type_: t.clean(cx),
                        id: 0,
                        name: names.next().unwrap_or("".to_string()),
                    }
                }).collect(),
            },
        }
    }
}

#[deriving(Clone, Encodable, Decodable, PartialEq)]
pub struct Argument {
    pub type_: Type,
    pub name: String,
    pub id: ast::NodeId,
}

impl Clean<Argument> for ast::Arg {
    fn clean(&self, cx: &DocContext) -> Argument {
        Argument {
            name: name_from_pat(&*self.pat),
            type_: (self.ty.clean(cx)),
            id: self.id
        }
    }
}

#[deriving(Clone, Encodable, Decodable, PartialEq)]
pub enum FunctionRetTy {
    Return(Type),
    NoReturn
}

impl Clean<FunctionRetTy> for ast::FunctionRetTy {
    fn clean(&self, cx: &DocContext) -> FunctionRetTy {
        match *self {
            ast::Return(ref typ) => Return(typ.clean(cx)),
            ast::NoReturn(_) => NoReturn
        }
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub struct Trait {
    pub items: Vec<TraitMethod>,
    pub generics: Generics,
    pub bounds: Vec<TyParamBound>,
    /// An optional default bound not required for `Self`, like `Sized?`
    pub default_unbound: Option<Type>
}

impl Clean<Item> for doctree::Trait {
    fn clean(&self, cx: &DocContext) -> Item {
        Item {
            name: Some(self.name.clean(cx)),
            attrs: self.attrs.clean(cx),
            source: self.whence.clean(cx),
            def_id: ast_util::local_def(self.id),
            visibility: self.vis.clean(cx),
            stability: self.stab.clean(cx),
            inner: TraitItem(Trait {
                items: self.items.clean(cx),
                generics: self.generics.clean(cx),
                bounds: self.bounds.clean(cx),
                default_unbound: self.default_unbound.clean(cx)
            }),
        }
    }
}

impl Clean<Type> for ast::TraitRef {
    fn clean(&self, cx: &DocContext) -> Type {
        resolve_type(cx, self.path.clean(cx), self.ref_id)
    }
}

impl Clean<Type> for ast::PolyTraitRef {
    fn clean(&self, cx: &DocContext) -> Type {
        self.trait_ref.clean(cx)
    }
}

/// An item belonging to a trait, whether a method or associated. Could be named
/// TraitItem except that's already taken by an exported enum variant.
#[deriving(Clone, Encodable, Decodable)]
pub enum TraitMethod {
    RequiredMethod(Item),
    ProvidedMethod(Item),
    TypeTraitItem(Item),
}

impl TraitMethod {
    pub fn is_req(&self) -> bool {
        match self {
            &RequiredMethod(..) => true,
            _ => false,
        }
    }
    pub fn is_def(&self) -> bool {
        match self {
            &ProvidedMethod(..) => true,
            _ => false,
        }
    }
    pub fn is_type(&self) -> bool {
        match self {
            &TypeTraitItem(..) => true,
            _ => false,
        }
    }
    pub fn item<'a>(&'a self) -> &'a Item {
        match *self {
            RequiredMethod(ref item) => item,
            ProvidedMethod(ref item) => item,
            TypeTraitItem(ref item) => item,
        }
    }
}

impl Clean<TraitMethod> for ast::TraitItem {
    fn clean(&self, cx: &DocContext) -> TraitMethod {
        match self {
            &ast::RequiredMethod(ref t) => RequiredMethod(t.clean(cx)),
            &ast::ProvidedMethod(ref t) => ProvidedMethod(t.clean(cx)),
            &ast::TypeTraitItem(ref t) => TypeTraitItem(t.clean(cx)),
        }
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub enum ImplMethod {
    MethodImplItem(Item),
    TypeImplItem(Item),
}

impl Clean<ImplMethod> for ast::ImplItem {
    fn clean(&self, cx: &DocContext) -> ImplMethod {
        match self {
            &ast::MethodImplItem(ref t) => MethodImplItem(t.clean(cx)),
            &ast::TypeImplItem(ref t) => TypeImplItem(t.clean(cx)),
        }
    }
}

impl<'tcx> Clean<Item> for ty::Method<'tcx> {
    fn clean(&self, cx: &DocContext) -> Item {
        let (self_, sig) = match self.explicit_self {
            ty::StaticExplicitSelfCategory => (ast::SelfStatic.clean(cx),
                                               self.fty.sig.clone()),
            s => {
                let sig = ty::FnSig {
                    inputs: self.fty.sig.inputs[1..].to_vec(),
                    ..self.fty.sig.clone()
                };
                let s = match s {
                    ty::ByValueExplicitSelfCategory => SelfValue,
                    ty::ByReferenceExplicitSelfCategory(..) => {
                        match self.fty.sig.inputs[0].sty {
                            ty::ty_rptr(r, mt) => {
                                SelfBorrowed(r.clean(cx), mt.mutbl.clean(cx))
                            }
                            _ => unreachable!(),
                        }
                    }
                    ty::ByBoxExplicitSelfCategory => {
                        SelfExplicit(self.fty.sig.inputs[0].clean(cx))
                    }
                    ty::StaticExplicitSelfCategory => unreachable!(),
                };
                (s, sig)
            }
        };

        Item {
            name: Some(self.name.clean(cx)),
            visibility: Some(ast::Inherited),
            stability: get_stability(cx, self.def_id),
            def_id: self.def_id,
            attrs: inline::load_attrs(cx, cx.tcx(), self.def_id),
            source: Span::empty(),
            inner: TyMethodItem(TyMethod {
                fn_style: self.fty.fn_style,
                generics: (&self.generics, subst::FnSpace).clean(cx),
                self_: self_,
                decl: (self.def_id, &sig).clean(cx),
            })
        }
    }
}

impl<'tcx> Clean<Item> for ty::ImplOrTraitItem<'tcx> {
    fn clean(&self, cx: &DocContext) -> Item {
        match *self {
            ty::MethodTraitItem(ref mti) => mti.clean(cx),
            ty::TypeTraitItem(ref tti) => tti.clean(cx),
        }
    }
}

/// A representation of a Type suitable for hyperlinking purposes. Ideally one can get the original
/// type out of the AST/ty::ctxt given one of these, if more information is needed. Most importantly
/// it does not preserve mutability or boxes.
#[deriving(Clone, Encodable, Decodable, PartialEq)]
pub enum Type {
    /// structs/enums/traits (anything that'd be an ast::TyPath)
    ResolvedPath {
        path: Path,
        typarams: Option<Vec<TyParamBound>>,
        did: ast::DefId,
    },
    // I have no idea how to usefully use this.
    TyParamBinder(ast::NodeId),
    /// For parameterized types, so the consumer of the JSON don't go looking
    /// for types which don't exist anywhere.
    Generic(ast::DefId),
    /// For references to self
    Self(ast::DefId),
    /// Primitives are just the fixed-size numeric types (plus int/uint/float), and char.
    Primitive(PrimitiveType),
    Closure(Box<ClosureDecl>),
    Proc(Box<ClosureDecl>),
    /// extern "ABI" fn
    BareFunction(Box<BareFunctionDecl>),
    Tuple(Vec<Type>),
    Vector(Box<Type>),
    FixedVector(Box<Type>, String),
    /// aka TyBot
    Bottom,
    Unique(Box<Type>),
    RawPointer(Mutability, Box<Type>),
    BorrowedRef {
        lifetime: Option<Lifetime>,
        mutability: Mutability,
        type_: Box<Type>,
    },
    QPath {
        name: String,
        self_type: Box<Type>,
        trait_: Box<Type>
    },
    // region, raw, other boxes, mutable
}

#[deriving(Clone, Encodable, Decodable, PartialEq, Eq, Hash)]
pub enum PrimitiveType {
    Int, I8, I16, I32, I64,
    Uint, U8, U16, U32, U64,
    F32, F64,
    Char,
    Bool,
    Str,
    Slice,
    PrimitiveTuple,
}

impl Copy for PrimitiveType {}

#[deriving(Clone, Encodable, Decodable)]
pub enum TypeKind {
    TypeEnum,
    TypeFunction,
    TypeModule,
    TypeStatic,
    TypeStruct,
    TypeTrait,
    TypeVariant,
    TypeTypedef,
}

impl Copy for TypeKind {}

impl PrimitiveType {
    fn from_str(s: &str) -> Option<PrimitiveType> {
        match s.as_slice() {
            "int" => Some(Int),
            "i8" => Some(I8),
            "i16" => Some(I16),
            "i32" => Some(I32),
            "i64" => Some(I64),
            "uint" => Some(Uint),
            "u8" => Some(U8),
            "u16" => Some(U16),
            "u32" => Some(U32),
            "u64" => Some(U64),
            "bool" => Some(Bool),
            "char" => Some(Char),
            "str" => Some(Str),
            "f32" => Some(F32),
            "f64" => Some(F64),
            "slice" => Some(Slice),
            "tuple" => Some(PrimitiveTuple),
            _ => None,
        }
    }

    fn find(attrs: &[Attribute]) -> Option<PrimitiveType> {
        for attr in attrs.iter() {
            let list = match *attr {
                List(ref k, ref l) if *k == "doc" => l,
                _ => continue,
            };
            for sub_attr in list.iter() {
                let value = match *sub_attr {
                    NameValue(ref k, ref v)
                        if *k == "primitive" => v.as_slice(),
                    _ => continue,
                };
                match PrimitiveType::from_str(value) {
                    Some(p) => return Some(p),
                    None => {}
                }
            }
        }
        return None
    }

    pub fn to_string(&self) -> &'static str {
        match *self {
            Int => "int",
            I8 => "i8",
            I16 => "i16",
            I32 => "i32",
            I64 => "i64",
            Uint => "uint",
            U8 => "u8",
            U16 => "u16",
            U32 => "u32",
            U64 => "u64",
            F32 => "f32",
            F64 => "f64",
            Str => "str",
            Bool => "bool",
            Char => "char",
            Slice => "slice",
            PrimitiveTuple => "tuple",
        }
    }

    pub fn to_url_str(&self) -> &'static str {
        self.to_string()
    }

    /// Creates a rustdoc-specific node id for primitive types.
    ///
    /// These node ids are generally never used by the AST itself.
    pub fn to_node_id(&self) -> ast::NodeId {
        u32::MAX - 1 - (*self as u32)
    }
}

impl Clean<Type> for ast::Ty {
    fn clean(&self, cx: &DocContext) -> Type {
        use syntax::ast::*;
        match self.node {
            TyPtr(ref m) => RawPointer(m.mutbl.clean(cx), box m.ty.clean(cx)),
            TyRptr(ref l, ref m) =>
                BorrowedRef {lifetime: l.clean(cx), mutability: m.mutbl.clean(cx),
                             type_: box m.ty.clean(cx)},
            TyVec(ref ty) => Vector(box ty.clean(cx)),
            TyFixedLengthVec(ref ty, ref e) => FixedVector(box ty.clean(cx),
                                                           e.span.to_src(cx)),
            TyTup(ref tys) => Tuple(tys.clean(cx)),
            TyPath(ref p, id) => {
                resolve_type(cx, p.clean(cx), id)
            }
            TyObjectSum(ref lhs, ref bounds) => {
                let lhs_ty = lhs.clean(cx);
                match lhs_ty {
                    ResolvedPath { path, typarams: None, did } => {
                        ResolvedPath { path: path, typarams: Some(bounds.clean(cx)), did: did}
                    }
                    _ => {
                        lhs_ty // shouldn't happen
                    }
                }
            }
            TyClosure(ref c) => Closure(box c.clean(cx)),
            TyProc(ref c) => Proc(box c.clean(cx)),
            TyBareFn(ref barefn) => BareFunction(box barefn.clean(cx)),
            TyParen(ref ty) => ty.clean(cx),
            TyQPath(ref qp) => qp.clean(cx),
            ref x => panic!("Unimplemented type {}", x),
        }
    }
}

impl<'tcx> Clean<Type> for ty::Ty<'tcx> {
    fn clean(&self, cx: &DocContext) -> Type {
        match self.sty {
            ty::ty_bool => Primitive(Bool),
            ty::ty_char => Primitive(Char),
            ty::ty_int(ast::TyI) => Primitive(Int),
            ty::ty_int(ast::TyI8) => Primitive(I8),
            ty::ty_int(ast::TyI16) => Primitive(I16),
            ty::ty_int(ast::TyI32) => Primitive(I32),
            ty::ty_int(ast::TyI64) => Primitive(I64),
            ty::ty_uint(ast::TyU) => Primitive(Uint),
            ty::ty_uint(ast::TyU8) => Primitive(U8),
            ty::ty_uint(ast::TyU16) => Primitive(U16),
            ty::ty_uint(ast::TyU32) => Primitive(U32),
            ty::ty_uint(ast::TyU64) => Primitive(U64),
            ty::ty_float(ast::TyF32) => Primitive(F32),
            ty::ty_float(ast::TyF64) => Primitive(F64),
            ty::ty_str => Primitive(Str),
            ty::ty_uniq(t) => {
                let box_did = cx.tcx_opt().and_then(|tcx| {
                    tcx.lang_items.owned_box()
                });
                lang_struct(cx, box_did, t, "Box", Unique)
            }
            ty::ty_vec(ty, None) => Vector(box ty.clean(cx)),
            ty::ty_vec(ty, Some(i)) => FixedVector(box ty.clean(cx),
                                                   format!("{}", i)),
            ty::ty_ptr(mt) => RawPointer(mt.mutbl.clean(cx), box mt.ty.clean(cx)),
            ty::ty_rptr(r, mt) => BorrowedRef {
                lifetime: r.clean(cx),
                mutability: mt.mutbl.clean(cx),
                type_: box mt.ty.clean(cx),
            },
            ty::ty_bare_fn(ref fty) => BareFunction(box BareFunctionDecl {
                fn_style: fty.fn_style,
                generics: Generics {
                    lifetimes: Vec::new(),
                    type_params: Vec::new(),
                    where_predicates: Vec::new()
                },
                decl: (ast_util::local_def(0), &fty.sig).clean(cx),
                abi: fty.abi.to_string(),
            }),
            ty::ty_closure(ref fty) => {
                let decl = box ClosureDecl {
                    lifetimes: Vec::new(), // FIXME: this looks wrong...
                    decl: (ast_util::local_def(0), &fty.sig).clean(cx),
                    onceness: fty.onceness,
                    fn_style: fty.fn_style,
                    bounds: fty.bounds.clean(cx),
                };
                match fty.store {
                    ty::UniqTraitStore => Proc(decl),
                    ty::RegionTraitStore(..) => Closure(decl),
                }
            }
            ty::ty_struct(did, ref substs) |
            ty::ty_enum(did, ref substs) |
            ty::ty_trait(box ty::TyTrait { principal: ty::TraitRef { def_id: did, ref substs },
                                           .. }) => {
                let fqn = csearch::get_item_path(cx.tcx(), did);
                let fqn: Vec<String> = fqn.into_iter().map(|i| {
                    i.to_string()
                }).collect();
                let kind = match self.sty {
                    ty::ty_struct(..) => TypeStruct,
                    ty::ty_trait(..) => TypeTrait,
                    _ => TypeEnum,
                };
                let path = external_path(cx, fqn.last().unwrap().to_string().as_slice(),
                                         substs);
                cx.external_paths.borrow_mut().as_mut().unwrap().insert(did, (fqn, kind));
                ResolvedPath {
                    path: path,
                    typarams: None,
                    did: did,
                }
            }
            ty::ty_tup(ref t) => Tuple(t.clean(cx)),

            ty::ty_param(ref p) => {
                if p.space == subst::SelfSpace {
                    Self(p.def_id)
                } else {
                    Generic(p.def_id)
                }
            }

            ty::ty_unboxed_closure(..) => Tuple(vec![]), // FIXME(pcwalton)

            ty::ty_infer(..) => panic!("ty_infer"),
            ty::ty_open(..) => panic!("ty_open"),
            ty::ty_err => panic!("ty_err"),
        }
    }
}

impl Clean<Type> for ast::QPath {
    fn clean(&self, cx: &DocContext) -> Type {
        Type::QPath {
            name: self.item_name.clean(cx),
            self_type: box self.self_type.clean(cx),
            trait_: box self.trait_ref.clean(cx)
        }
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub enum StructField {
    HiddenStructField, // inserted later by strip passes
    TypedStructField(Type),
}

impl Clean<Item> for ast::StructField {
    fn clean(&self, cx: &DocContext) -> Item {
        let (name, vis) = match self.node.kind {
            ast::NamedField(id, vis) => (Some(id), vis),
            ast::UnnamedField(vis) => (None, vis)
        };
        Item {
            name: name.clean(cx),
            attrs: self.node.attrs.clean(cx),
            source: self.span.clean(cx),
            visibility: Some(vis),
            stability: get_stability(cx, ast_util::local_def(self.node.id)),
            def_id: ast_util::local_def(self.node.id),
            inner: StructFieldItem(TypedStructField(self.node.ty.clean(cx))),
        }
    }
}

impl Clean<Item> for ty::field_ty {
    fn clean(&self, cx: &DocContext) -> Item {
        use syntax::parse::token::special_idents::unnamed_field;
        use rustc::metadata::csearch;

        let attr_map = csearch::get_struct_field_attrs(&cx.tcx().sess.cstore, self.id);

        let (name, attrs) = if self.name == unnamed_field.name {
            (None, None)
        } else {
            (Some(self.name), Some(attr_map.get(&self.id.node).unwrap()))
        };

        let ty = ty::lookup_item_type(cx.tcx(), self.id);

        Item {
            name: name.clean(cx),
            attrs: attrs.unwrap_or(&Vec::new()).clean(cx),
            source: Span::empty(),
            visibility: Some(self.vis),
            stability: get_stability(cx, self.id),
            def_id: self.id,
            inner: StructFieldItem(TypedStructField(ty.ty.clean(cx))),
        }
    }
}

pub type Visibility = ast::Visibility;

impl Clean<Option<Visibility>> for ast::Visibility {
    fn clean(&self, _: &DocContext) -> Option<Visibility> {
        Some(*self)
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub struct Struct {
    pub struct_type: doctree::StructType,
    pub generics: Generics,
    pub fields: Vec<Item>,
    pub fields_stripped: bool,
}

impl Clean<Item> for doctree::Struct {
    fn clean(&self, cx: &DocContext) -> Item {
        Item {
            name: Some(self.name.clean(cx)),
            attrs: self.attrs.clean(cx),
            source: self.whence.clean(cx),
            def_id: ast_util::local_def(self.id),
            visibility: self.vis.clean(cx),
            stability: self.stab.clean(cx),
            inner: StructItem(Struct {
                struct_type: self.struct_type,
                generics: self.generics.clean(cx),
                fields: self.fields.clean(cx),
                fields_stripped: false,
            }),
        }
    }
}

/// This is a more limited form of the standard Struct, different in that
/// it lacks the things most items have (name, id, parameterization). Found
/// only as a variant in an enum.
#[deriving(Clone, Encodable, Decodable)]
pub struct VariantStruct {
    pub struct_type: doctree::StructType,
    pub fields: Vec<Item>,
    pub fields_stripped: bool,
}

impl Clean<VariantStruct> for syntax::ast::StructDef {
    fn clean(&self, cx: &DocContext) -> VariantStruct {
        VariantStruct {
            struct_type: doctree::struct_type_from_def(self),
            fields: self.fields.clean(cx),
            fields_stripped: false,
        }
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub struct Enum {
    pub variants: Vec<Item>,
    pub generics: Generics,
    pub variants_stripped: bool,
}

impl Clean<Item> for doctree::Enum {
    fn clean(&self, cx: &DocContext) -> Item {
        Item {
            name: Some(self.name.clean(cx)),
            attrs: self.attrs.clean(cx),
            source: self.whence.clean(cx),
            def_id: ast_util::local_def(self.id),
            visibility: self.vis.clean(cx),
            stability: self.stab.clean(cx),
            inner: EnumItem(Enum {
                variants: self.variants.clean(cx),
                generics: self.generics.clean(cx),
                variants_stripped: false,
            }),
        }
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub struct Variant {
    pub kind: VariantKind,
}

impl Clean<Item> for doctree::Variant {
    fn clean(&self, cx: &DocContext) -> Item {
        Item {
            name: Some(self.name.clean(cx)),
            attrs: self.attrs.clean(cx),
            source: self.whence.clean(cx),
            visibility: self.vis.clean(cx),
            stability: self.stab.clean(cx),
            def_id: ast_util::local_def(self.id),
            inner: VariantItem(Variant {
                kind: self.kind.clean(cx),
            }),
        }
    }
}

impl<'tcx> Clean<Item> for ty::VariantInfo<'tcx> {
    fn clean(&self, cx: &DocContext) -> Item {
        // use syntax::parse::token::special_idents::unnamed_field;
        let kind = match self.arg_names.as_ref().map(|s| s.as_slice()) {
            None | Some([]) if self.args.len() == 0 => CLikeVariant,
            None | Some([]) => {
                TupleVariant(self.args.clean(cx))
            }
            Some(s) => {
                StructVariant(VariantStruct {
                    struct_type: doctree::Plain,
                    fields_stripped: false,
                    fields: s.iter().zip(self.args.iter()).map(|(name, ty)| {
                        Item {
                            source: Span::empty(),
                            name: Some(name.clean(cx)),
                            attrs: Vec::new(),
                            visibility: Some(ast::Public),
                            // FIXME: this is not accurate, we need an id for
                            //        the specific field but we're using the id
                            //        for the whole variant. Thus we read the
                            //        stability from the whole variant as well.
                            //        Struct variants are experimental and need
                            //        more infrastructure work before we can get
                            //        at the needed information here.
                            def_id: self.id,
                            stability: get_stability(cx, self.id),
                            inner: StructFieldItem(
                                TypedStructField(ty.clean(cx))
                            )
                        }
                    }).collect()
                })
            }
        };
        Item {
            name: Some(self.name.clean(cx)),
            attrs: inline::load_attrs(cx, cx.tcx(), self.id),
            source: Span::empty(),
            visibility: Some(ast::Public),
            def_id: self.id,
            inner: VariantItem(Variant { kind: kind }),
            stability: get_stability(cx, self.id),
        }
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub enum VariantKind {
    CLikeVariant,
    TupleVariant(Vec<Type>),
    StructVariant(VariantStruct),
}

impl Clean<VariantKind> for ast::VariantKind {
    fn clean(&self, cx: &DocContext) -> VariantKind {
        match self {
            &ast::TupleVariantKind(ref args) => {
                if args.len() == 0 {
                    CLikeVariant
                } else {
                    TupleVariant(args.iter().map(|x| x.ty.clean(cx)).collect())
                }
            },
            &ast::StructVariantKind(ref sd) => StructVariant(sd.clean(cx)),
        }
    }
}

#[deriving(Clone, Encodable, Decodable, Show)]
pub struct Span {
    pub filename: String,
    pub loline: uint,
    pub locol: uint,
    pub hiline: uint,
    pub hicol: uint,
}

impl Span {
    fn empty() -> Span {
        Span {
            filename: "".to_string(),
            loline: 0, locol: 0,
            hiline: 0, hicol: 0,
        }
    }
}

impl Clean<Span> for syntax::codemap::Span {
    fn clean(&self, cx: &DocContext) -> Span {
        let cm = cx.sess().codemap();
        let filename = cm.span_to_filename(*self);
        let lo = cm.lookup_char_pos(self.lo);
        let hi = cm.lookup_char_pos(self.hi);
        Span {
            filename: filename.to_string(),
            loline: lo.line,
            locol: lo.col.to_uint(),
            hiline: hi.line,
            hicol: hi.col.to_uint(),
        }
    }
}

#[deriving(Clone, Encodable, Decodable, PartialEq)]
pub struct Path {
    pub global: bool,
    pub segments: Vec<PathSegment>,
}

impl Clean<Path> for ast::Path {
    fn clean(&self, cx: &DocContext) -> Path {
        Path {
            global: self.global,
            segments: self.segments.clean(cx),
        }
    }
}

#[deriving(Clone, Encodable, Decodable, PartialEq)]
pub struct PathSegment {
    pub name: String,
    pub lifetimes: Vec<Lifetime>,
    pub types: Vec<Type>,
}

impl Clean<PathSegment> for ast::PathSegment {
    fn clean(&self, cx: &DocContext) -> PathSegment {
        let (lifetimes, types) = match self.parameters {
            ast::AngleBracketedParameters(ref data) => {
                (data.lifetimes.clean(cx), data.types.clean(cx))
            }

            ast::ParenthesizedParameters(ref data) => {
                // FIXME -- rustdoc should be taught about Foo() notation
                let inputs = Tuple(data.inputs.clean(cx));
                let output = data.output.as_ref().map(|t| t.clean(cx)).unwrap_or(Tuple(Vec::new()));
                (Vec::new(), vec![inputs, output])
            }
        };

        PathSegment {
            name: self.identifier.clean(cx),
            lifetimes: lifetimes,
            types: types,
        }
    }
}

fn path_to_string(p: &ast::Path) -> String {
    let mut s = String::new();
    let mut first = true;
    for i in p.segments.iter().map(|x| token::get_ident(x.identifier)) {
        if !first || p.global {
            s.push_str("::");
        } else {
            first = false;
        }
        s.push_str(i.get());
    }
    s
}

impl Clean<String> for ast::Ident {
    fn clean(&self, _: &DocContext) -> String {
        token::get_ident(*self).get().to_string()
    }
}

impl Clean<String> for ast::Name {
    fn clean(&self, _: &DocContext) -> String {
        token::get_name(*self).get().to_string()
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub struct Typedef {
    pub type_: Type,
    pub generics: Generics,
}

impl Clean<Item> for doctree::Typedef {
    fn clean(&self, cx: &DocContext) -> Item {
        Item {
            name: Some(self.name.clean(cx)),
            attrs: self.attrs.clean(cx),
            source: self.whence.clean(cx),
            def_id: ast_util::local_def(self.id.clone()),
            visibility: self.vis.clean(cx),
            stability: self.stab.clean(cx),
            inner: TypedefItem(Typedef {
                type_: self.ty.clean(cx),
                generics: self.gen.clean(cx),
            }),
        }
    }
}

#[deriving(Clone, Encodable, Decodable, PartialEq)]
pub struct BareFunctionDecl {
    pub fn_style: ast::FnStyle,
    pub generics: Generics,
    pub decl: FnDecl,
    pub abi: String,
}

impl Clean<BareFunctionDecl> for ast::BareFnTy {
    fn clean(&self, cx: &DocContext) -> BareFunctionDecl {
        BareFunctionDecl {
            fn_style: self.fn_style,
            generics: Generics {
                lifetimes: self.lifetimes.clean(cx),
                type_params: Vec::new(),
                where_predicates: Vec::new()
            },
            decl: self.decl.clean(cx),
            abi: self.abi.to_string(),
        }
    }
}

#[deriving(Clone, Encodable, Decodable, Show)]
pub struct Static {
    pub type_: Type,
    pub mutability: Mutability,
    /// It's useful to have the value of a static documented, but I have no
    /// desire to represent expressions (that'd basically be all of the AST,
    /// which is huge!). So, have a string.
    pub expr: String,
}

impl Clean<Item> for doctree::Static {
    fn clean(&self, cx: &DocContext) -> Item {
        debug!("claning static {}: {}", self.name.clean(cx), self);
        Item {
            name: Some(self.name.clean(cx)),
            attrs: self.attrs.clean(cx),
            source: self.whence.clean(cx),
            def_id: ast_util::local_def(self.id),
            visibility: self.vis.clean(cx),
            stability: self.stab.clean(cx),
            inner: StaticItem(Static {
                type_: self.type_.clean(cx),
                mutability: self.mutability.clean(cx),
                expr: self.expr.span.to_src(cx),
            }),
        }
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub struct Constant {
    pub type_: Type,
    pub expr: String,
}

impl Clean<Item> for doctree::Constant {
    fn clean(&self, cx: &DocContext) -> Item {
        Item {
            name: Some(self.name.clean(cx)),
            attrs: self.attrs.clean(cx),
            source: self.whence.clean(cx),
            def_id: ast_util::local_def(self.id),
            visibility: self.vis.clean(cx),
            stability: self.stab.clean(cx),
            inner: ConstantItem(Constant {
                type_: self.type_.clean(cx),
                expr: self.expr.span.to_src(cx),
            }),
        }
    }
}

#[deriving(Show, Clone, Encodable, Decodable, PartialEq)]
pub enum Mutability {
    Mutable,
    Immutable,
}

impl Copy for Mutability {}

impl Clean<Mutability> for ast::Mutability {
    fn clean(&self, _: &DocContext) -> Mutability {
        match self {
            &ast::MutMutable => Mutable,
            &ast::MutImmutable => Immutable,
        }
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub struct Impl {
    pub generics: Generics,
    pub trait_: Option<Type>,
    pub for_: Type,
    pub items: Vec<Item>,
    pub derived: bool,
}

fn detect_derived<M: AttrMetaMethods>(attrs: &[M]) -> bool {
    attr::contains_name(attrs, "automatically_derived")
}

impl Clean<Item> for doctree::Impl {
    fn clean(&self, cx: &DocContext) -> Item {
        Item {
            name: None,
            attrs: self.attrs.clean(cx),
            source: self.whence.clean(cx),
            def_id: ast_util::local_def(self.id),
            visibility: self.vis.clean(cx),
            stability: self.stab.clean(cx),
            inner: ImplItem(Impl {
                generics: self.generics.clean(cx),
                trait_: self.trait_.clean(cx),
                for_: self.for_.clean(cx),
                items: self.items.clean(cx).into_iter().map(|ti| {
                        match ti {
                            MethodImplItem(i) => i,
                            TypeImplItem(i) => i,
                        }
                    }).collect(),
                derived: detect_derived(self.attrs.as_slice()),
            }),
        }
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub struct ViewItem {
    pub inner: ViewItemInner,
}

impl Clean<Vec<Item>> for ast::ViewItem {
    fn clean(&self, cx: &DocContext) -> Vec<Item> {
        // We consider inlining the documentation of `pub use` statements, but we
        // forcefully don't inline if this is not public or if the
        // #[doc(no_inline)] attribute is present.
        let denied = self.vis != ast::Public || self.attrs.iter().any(|a| {
            a.name().get() == "doc" && match a.meta_item_list() {
                Some(l) => attr::contains_name(l, "no_inline"),
                None => false,
            }
        });
        let convert = |node: &ast::ViewItem_| {
            Item {
                name: None,
                attrs: self.attrs.clean(cx),
                source: self.span.clean(cx),
                def_id: ast_util::local_def(0),
                visibility: self.vis.clean(cx),
                stability: None,
                inner: ViewItemItem(ViewItem { inner: node.clean(cx) }),
            }
        };
        let mut ret = Vec::new();
        match self.node {
            ast::ViewItemUse(ref path) if !denied => {
                match path.node {
                    ast::ViewPathGlob(..) => ret.push(convert(&self.node)),
                    ast::ViewPathList(ref a, ref list, ref b) => {
                        // Attempt to inline all reexported items, but be sure
                        // to keep any non-inlineable reexports so they can be
                        // listed in the documentation.
                        let remaining = list.iter().filter(|path| {
                            match inline::try_inline(cx, path.node.id(), None) {
                                Some(items) => {
                                    ret.extend(items.into_iter()); false
                                }
                                None => true,
                            }
                        }).map(|a| a.clone()).collect::<Vec<ast::PathListItem>>();
                        if remaining.len() > 0 {
                            let path = ast::ViewPathList(a.clone(),
                                                         remaining,
                                                         b.clone());
                            let path = syntax::codemap::dummy_spanned(path);
                            ret.push(convert(&ast::ViewItemUse(P(path))));
                        }
                    }
                    ast::ViewPathSimple(ident, _, id) => {
                        match inline::try_inline(cx, id, Some(ident)) {
                            Some(items) => ret.extend(items.into_iter()),
                            None => ret.push(convert(&self.node)),
                        }
                    }
                }
            }
            ref n => ret.push(convert(n)),
        }
        return ret;
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub enum ViewItemInner {
    ExternCrate(String, Option<String>, ast::NodeId),
    Import(ViewPath)
}

impl Clean<ViewItemInner> for ast::ViewItem_ {
    fn clean(&self, cx: &DocContext) -> ViewItemInner {
        match self {
            &ast::ViewItemExternCrate(ref i, ref p, ref id) => {
                let string = match *p {
                    None => None,
                    Some((ref x, _)) => Some(x.get().to_string()),
                };
                ExternCrate(i.clean(cx), string, *id)
            }
            &ast::ViewItemUse(ref vp) => {
                Import(vp.clean(cx))
            }
        }
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub enum ViewPath {
    // use source as str;
    SimpleImport(String, ImportSource),
    // use source::*;
    GlobImport(ImportSource),
    // use source::{a, b, c};
    ImportList(ImportSource, Vec<ViewListIdent>),
}

#[deriving(Clone, Encodable, Decodable)]
pub struct ImportSource {
    pub path: Path,
    pub did: Option<ast::DefId>,
}

impl Clean<ViewPath> for ast::ViewPath {
    fn clean(&self, cx: &DocContext) -> ViewPath {
        match self.node {
            ast::ViewPathSimple(ref i, ref p, id) =>
                SimpleImport(i.clean(cx), resolve_use_source(cx, p.clean(cx), id)),
            ast::ViewPathGlob(ref p, id) =>
                GlobImport(resolve_use_source(cx, p.clean(cx), id)),
            ast::ViewPathList(ref p, ref pl, id) => {
                ImportList(resolve_use_source(cx, p.clean(cx), id),
                           pl.clean(cx))
            }
        }
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub struct ViewListIdent {
    pub name: String,
    pub source: Option<ast::DefId>,
}

impl Clean<ViewListIdent> for ast::PathListItem {
    fn clean(&self, cx: &DocContext) -> ViewListIdent {
        match self.node {
            ast::PathListIdent { id, name } => ViewListIdent {
                name: name.clean(cx),
                source: resolve_def(cx, id)
            },
            ast::PathListMod { id } => ViewListIdent {
                name: "mod".to_string(),
                source: resolve_def(cx, id)
            }
        }
    }
}

impl Clean<Vec<Item>> for ast::ForeignMod {
    fn clean(&self, cx: &DocContext) -> Vec<Item> {
        self.items.clean(cx)
    }
}

impl Clean<Item> for ast::ForeignItem {
    fn clean(&self, cx: &DocContext) -> Item {
        let inner = match self.node {
            ast::ForeignItemFn(ref decl, ref generics) => {
                ForeignFunctionItem(Function {
                    decl: decl.clean(cx),
                    generics: generics.clean(cx),
                    fn_style: ast::UnsafeFn,
                })
            }
            ast::ForeignItemStatic(ref ty, mutbl) => {
                ForeignStaticItem(Static {
                    type_: ty.clean(cx),
                    mutability: if mutbl {Mutable} else {Immutable},
                    expr: "".to_string(),
                })
            }
        };
        Item {
            name: Some(self.ident.clean(cx)),
            attrs: self.attrs.clean(cx),
            source: self.span.clean(cx),
            def_id: ast_util::local_def(self.id),
            visibility: self.vis.clean(cx),
            stability: get_stability(cx, ast_util::local_def(self.id)),
            inner: inner,
        }
    }
}

// Utilities

trait ToSource {
    fn to_src(&self, cx: &DocContext) -> String;
}

impl ToSource for syntax::codemap::Span {
    fn to_src(&self, cx: &DocContext) -> String {
        debug!("converting span {} to snippet", self.clean(cx));
        let sn = match cx.sess().codemap().span_to_snippet(*self) {
            Some(x) => x.to_string(),
            None    => "".to_string()
        };
        debug!("got snippet {}", sn);
        sn
    }
}

fn lit_to_string(lit: &ast::Lit) -> String {
    match lit.node {
        ast::LitStr(ref st, _) => st.get().to_string(),
        ast::LitBinary(ref data) => format!("{}", data),
        ast::LitByte(b) => {
            let mut res = String::from_str("b'");
            for c in (b as char).escape_default() {
                res.push(c);
            }
            res.push('\'');
            res
        },
        ast::LitChar(c) => format!("'{}'", c),
        ast::LitInt(i, _t) => i.to_string(),
        ast::LitFloat(ref f, _t) => f.get().to_string(),
        ast::LitFloatUnsuffixed(ref f) => f.get().to_string(),
        ast::LitBool(b) => b.to_string(),
    }
}

fn name_from_pat(p: &ast::Pat) -> String {
    use syntax::ast::*;
    debug!("Trying to get a name from pattern: {}", p);

    match p.node {
        PatWild(PatWildSingle) => "_".to_string(),
        PatWild(PatWildMulti) => "..".to_string(),
        PatIdent(_, ref p, _) => token::get_ident(p.node).get().to_string(),
        PatEnum(ref p, _) => path_to_string(p),
        PatStruct(ref name, ref fields, etc) => {
            format!("{} {{ {}{} }}", path_to_string(name),
                fields.iter().map(|&Spanned { node: ref fp, .. }|
                                  format!("{}: {}", fp.ident.as_str(), name_from_pat(&*fp.pat)))
                             .collect::<Vec<String>>().connect(", "),
                if etc { ", ..." } else { "" }
            )
        },
        PatTup(ref elts) => format!("({})", elts.iter().map(|p| name_from_pat(&**p))
                                            .collect::<Vec<String>>().connect(", ")),
        PatBox(ref p) => name_from_pat(&**p),
        PatRegion(ref p) => name_from_pat(&**p),
        PatLit(..) => {
            warn!("tried to get argument name from PatLit, \
                  which is silly in function arguments");
            "()".to_string()
        },
        PatRange(..) => panic!("tried to get argument name from PatRange, \
                              which is not allowed in function arguments"),
        PatVec(..) => panic!("tried to get argument name from pat_vec, \
                             which is not allowed in function arguments"),
        PatMac(..) => {
            warn!("can't document the name of a function argument \
                   produced by a pattern macro");
            "(argument produced by macro)".to_string()
        }
    }
}

/// Given a Type, resolve it using the def_map
fn resolve_type(cx: &DocContext,
                path: Path,
                id: ast::NodeId) -> Type {
    let tcx = match cx.tcx_opt() {
        Some(tcx) => tcx,
        // If we're extracting tests, this return value doesn't matter.
        None => return Primitive(Bool),
    };
    debug!("searching for {} in defmap", id);
    let def = match tcx.def_map.borrow().get(&id) {
        Some(&k) => k,
        None => panic!("unresolved id not in defmap")
    };

    match def {
        def::DefSelfTy(i) => return Self(ast_util::local_def(i)),
        def::DefPrimTy(p) => match p {
            ast::TyStr => return Primitive(Str),
            ast::TyBool => return Primitive(Bool),
            ast::TyChar => return Primitive(Char),
            ast::TyInt(ast::TyI) => return Primitive(Int),
            ast::TyInt(ast::TyI8) => return Primitive(I8),
            ast::TyInt(ast::TyI16) => return Primitive(I16),
            ast::TyInt(ast::TyI32) => return Primitive(I32),
            ast::TyInt(ast::TyI64) => return Primitive(I64),
            ast::TyUint(ast::TyU) => return Primitive(Uint),
            ast::TyUint(ast::TyU8) => return Primitive(U8),
            ast::TyUint(ast::TyU16) => return Primitive(U16),
            ast::TyUint(ast::TyU32) => return Primitive(U32),
            ast::TyUint(ast::TyU64) => return Primitive(U64),
            ast::TyFloat(ast::TyF32) => return Primitive(F32),
            ast::TyFloat(ast::TyF64) => return Primitive(F64),
        },
        def::DefTyParam(_, i, _) => return Generic(i),
        def::DefTyParamBinder(i) => return TyParamBinder(i),
        _ => {}
    };
    let did = register_def(&*cx, def);
    ResolvedPath { path: path, typarams: None, did: did }
}

fn register_def(cx: &DocContext, def: def::Def) -> ast::DefId {
    let (did, kind) = match def {
        def::DefFn(i, _) => (i, TypeFunction),
        def::DefTy(i, false) => (i, TypeTypedef),
        def::DefTy(i, true) => (i, TypeEnum),
        def::DefTrait(i) => (i, TypeTrait),
        def::DefStruct(i) => (i, TypeStruct),
        def::DefMod(i) => (i, TypeModule),
        def::DefStatic(i, _) => (i, TypeStatic),
        def::DefVariant(i, _, _) => (i, TypeEnum),
        _ => return def.def_id()
    };
    if ast_util::is_local(did) { return did }
    let tcx = match cx.tcx_opt() {
        Some(tcx) => tcx,
        None => return did
    };
    inline::record_extern_fqn(cx, did, kind);
    if let TypeTrait = kind {
        let t = inline::build_external_trait(cx, tcx, did);
        cx.external_traits.borrow_mut().as_mut().unwrap().insert(did, t);
    }
    return did;
}

fn resolve_use_source(cx: &DocContext, path: Path, id: ast::NodeId) -> ImportSource {
    ImportSource {
        path: path,
        did: resolve_def(cx, id),
    }
}

fn resolve_def(cx: &DocContext, id: ast::NodeId) -> Option<ast::DefId> {
    cx.tcx_opt().and_then(|tcx| {
        tcx.def_map.borrow().get(&id).map(|&def| register_def(cx, def))
    })
}

#[deriving(Clone, Encodable, Decodable)]
pub struct Macro {
    pub source: String,
}

impl Clean<Item> for doctree::Macro {
    fn clean(&self, cx: &DocContext) -> Item {
        Item {
            name: Some(format!("{}!", self.name.clean(cx))),
            attrs: self.attrs.clean(cx),
            source: self.whence.clean(cx),
            visibility: ast::Public.clean(cx),
            stability: self.stab.clean(cx),
            def_id: ast_util::local_def(self.id),
            inner: MacroItem(Macro {
                source: self.whence.to_src(cx),
            }),
        }
    }
}

#[deriving(Clone, Encodable, Decodable)]
pub struct Stability {
    pub level: attr::StabilityLevel,
    pub text: String
}

impl Clean<Stability> for attr::Stability {
    fn clean(&self, _: &DocContext) -> Stability {
        Stability {
            level: self.level,
            text: self.text.as_ref().map_or("".to_string(),
                                            |interned| interned.get().to_string()),
        }
    }
}

impl Clean<Item> for ast::AssociatedType {
    fn clean(&self, cx: &DocContext) -> Item {
        Item {
            source: self.ty_param.span.clean(cx),
            name: Some(self.ty_param.ident.clean(cx)),
            attrs: self.attrs.clean(cx),
            inner: AssociatedTypeItem(self.ty_param.clean(cx)),
            visibility: None,
            def_id: ast_util::local_def(self.ty_param.id),
            stability: None,
        }
    }
}

impl Clean<Item> for ty::AssociatedType {
    fn clean(&self, cx: &DocContext) -> Item {
        Item {
            source: DUMMY_SP.clean(cx),
            name: Some(self.name.clean(cx)),
            attrs: Vec::new(),
            // FIXME(#18048): this is wrong, but cross-crate associated types are broken
            // anyway, for the time being.
            inner: AssociatedTypeItem(TyParam {
                name: self.name.clean(cx),
                did: ast::DefId {
                    krate: 0,
                    node: ast::DUMMY_NODE_ID
                },
                bounds: vec![],
                default: None,
                default_unbound: None
            }),
            visibility: None,
            def_id: self.def_id,
            stability: None,
        }
    }
}

impl Clean<Item> for ast::Typedef {
    fn clean(&self, cx: &DocContext) -> Item {
        Item {
            source: self.span.clean(cx),
            name: Some(self.ident.clean(cx)),
            attrs: self.attrs.clean(cx),
            inner: TypedefItem(Typedef {
                type_: self.typ.clean(cx),
                generics: Generics {
                    lifetimes: Vec::new(),
                    type_params: Vec::new(),
                    where_predicates: Vec::new()
                },
            }),
            visibility: None,
            def_id: ast_util::local_def(self.id),
            stability: None,
        }
    }
}

fn lang_struct(cx: &DocContext, did: Option<ast::DefId>,
               t: ty::Ty, name: &str,
               fallback: fn(Box<Type>) -> Type) -> Type {
    let did = match did {
        Some(did) => did,
        None => return fallback(box t.clean(cx)),
    };
    let fqn = csearch::get_item_path(cx.tcx(), did);
    let fqn: Vec<String> = fqn.into_iter().map(|i| {
        i.to_string()
    }).collect();
    cx.external_paths.borrow_mut().as_mut().unwrap().insert(did, (fqn, TypeStruct));
    ResolvedPath {
        typarams: None,
        did: did,
        path: Path {
            global: false,
            segments: vec![PathSegment {
                name: name.to_string(),
                lifetimes: vec![],
                types: vec![t.clean(cx)],
            }],
        },
    }
}
