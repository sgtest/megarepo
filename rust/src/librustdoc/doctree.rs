// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! This module is used to store stuff from Rust's AST in a more convenient
//! manner (and with prettier names) before cleaning.
pub use self::StructType::*;
pub use self::TypeBound::*;

use syntax;
use syntax::codemap::Span;
use syntax::abi;
use syntax::ast;
use syntax::ast::{Ident, NodeId};
use syntax::attr;
use syntax::ptr::P;
use rustc_front::hir;

pub struct Module {
    pub name: Option<Ident>,
    pub attrs: Vec<ast::Attribute>,
    pub where_outer: Span,
    pub where_inner: Span,
    pub extern_crates: Vec<ExternCrate>,
    pub imports: Vec<Import>,
    pub structs: Vec<Struct>,
    pub enums: Vec<Enum>,
    pub fns: Vec<Function>,
    pub mods: Vec<Module>,
    pub id: NodeId,
    pub typedefs: Vec<Typedef>,
    pub statics: Vec<Static>,
    pub constants: Vec<Constant>,
    pub traits: Vec<Trait>,
    pub vis: hir::Visibility,
    pub stab: Option<attr::Stability>,
    pub impls: Vec<Impl>,
    pub def_traits: Vec<DefaultImpl>,
    pub foreigns: Vec<hir::ForeignMod>,
    pub macros: Vec<Macro>,
    pub is_crate: bool,
}

impl Module {
    pub fn new(name: Option<Ident>) -> Module {
        Module {
            name       : name,
            id: 0,
            vis: hir::Inherited,
            stab: None,
            where_outer: syntax::codemap::DUMMY_SP,
            where_inner: syntax::codemap::DUMMY_SP,
            attrs      : Vec::new(),
            extern_crates: Vec::new(),
            imports    : Vec::new(),
            structs    : Vec::new(),
            enums      : Vec::new(),
            fns        : Vec::new(),
            mods       : Vec::new(),
            typedefs   : Vec::new(),
            statics    : Vec::new(),
            constants  : Vec::new(),
            traits     : Vec::new(),
            impls      : Vec::new(),
            def_traits : Vec::new(),
            foreigns   : Vec::new(),
            macros     : Vec::new(),
            is_crate   : false,
        }
    }
}

#[derive(Debug, Clone, RustcEncodable, RustcDecodable, Copy)]
pub enum StructType {
    /// A normal struct
    Plain,
    /// A tuple struct
    Tuple,
    /// A newtype struct (tuple struct with one element)
    Newtype,
    /// A unit struct
    Unit
}

pub enum TypeBound {
    RegionBound,
    TraitBound(hir::TraitRef)
}

pub struct Struct {
    pub vis: hir::Visibility,
    pub stab: Option<attr::Stability>,
    pub id: NodeId,
    pub struct_type: StructType,
    pub name: Ident,
    pub generics: hir::Generics,
    pub attrs: Vec<ast::Attribute>,
    pub fields: Vec<hir::StructField>,
    pub whence: Span,
}

pub struct Enum {
    pub vis: hir::Visibility,
    pub stab: Option<attr::Stability>,
    pub variants: Vec<Variant>,
    pub generics: hir::Generics,
    pub attrs: Vec<ast::Attribute>,
    pub id: NodeId,
    pub whence: Span,
    pub name: Ident,
}

pub struct Variant {
    pub name: Ident,
    pub attrs: Vec<ast::Attribute>,
    pub kind: hir::VariantKind,
    pub id: ast::NodeId,
    pub vis: hir::Visibility,
    pub stab: Option<attr::Stability>,
    pub whence: Span,
}

pub struct Function {
    pub decl: hir::FnDecl,
    pub attrs: Vec<ast::Attribute>,
    pub id: NodeId,
    pub name: Ident,
    pub vis: hir::Visibility,
    pub stab: Option<attr::Stability>,
    pub unsafety: hir::Unsafety,
    pub constness: hir::Constness,
    pub whence: Span,
    pub generics: hir::Generics,
    pub abi: abi::Abi,
}

pub struct Typedef {
    pub ty: P<hir::Ty>,
    pub gen: hir::Generics,
    pub name: Ident,
    pub id: ast::NodeId,
    pub attrs: Vec<ast::Attribute>,
    pub whence: Span,
    pub vis: hir::Visibility,
    pub stab: Option<attr::Stability>,
}

#[derive(Debug)]
pub struct Static {
    pub type_: P<hir::Ty>,
    pub mutability: hir::Mutability,
    pub expr: P<hir::Expr>,
    pub name: Ident,
    pub attrs: Vec<ast::Attribute>,
    pub vis: hir::Visibility,
    pub stab: Option<attr::Stability>,
    pub id: ast::NodeId,
    pub whence: Span,
}

pub struct Constant {
    pub type_: P<hir::Ty>,
    pub expr: P<hir::Expr>,
    pub name: Ident,
    pub attrs: Vec<ast::Attribute>,
    pub vis: hir::Visibility,
    pub stab: Option<attr::Stability>,
    pub id: ast::NodeId,
    pub whence: Span,
}

pub struct Trait {
    pub unsafety: hir::Unsafety,
    pub name: Ident,
    pub items: Vec<P<hir::TraitItem>>, //should be TraitItem
    pub generics: hir::Generics,
    pub bounds: Vec<hir::TyParamBound>,
    pub attrs: Vec<ast::Attribute>,
    pub id: ast::NodeId,
    pub whence: Span,
    pub vis: hir::Visibility,
    pub stab: Option<attr::Stability>,
}

pub struct Impl {
    pub unsafety: hir::Unsafety,
    pub polarity: hir::ImplPolarity,
    pub generics: hir::Generics,
    pub trait_: Option<hir::TraitRef>,
    pub for_: P<hir::Ty>,
    pub items: Vec<P<hir::ImplItem>>,
    pub attrs: Vec<ast::Attribute>,
    pub whence: Span,
    pub vis: hir::Visibility,
    pub stab: Option<attr::Stability>,
    pub id: ast::NodeId,
}

pub struct DefaultImpl {
    pub unsafety: hir::Unsafety,
    pub trait_: hir::TraitRef,
    pub id: ast::NodeId,
    pub attrs: Vec<ast::Attribute>,
    pub whence: Span,
}

pub struct Macro {
    pub name: Ident,
    pub id: ast::NodeId,
    pub attrs: Vec<ast::Attribute>,
    pub whence: Span,
    pub stab: Option<attr::Stability>,
    pub imported_from: Option<Ident>,
}

pub struct ExternCrate {
    pub name: Ident,
    pub path: Option<String>,
    pub vis: hir::Visibility,
    pub attrs: Vec<ast::Attribute>,
    pub whence: Span,
}

pub struct Import {
    pub id: NodeId,
    pub vis: hir::Visibility,
    pub attrs: Vec<ast::Attribute>,
    pub node: hir::ViewPath_,
    pub whence: Span,
}

pub fn struct_type_from_def(sd: &hir::StructDef) -> StructType {
    if sd.ctor_id.is_some() {
        // We are in a tuple-struct
        match sd.fields.len() {
            0 => Unit,
            1 => Newtype,
            _ => Tuple
        }
    } else {
        Plain
    }
}
