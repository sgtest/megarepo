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
use syntax::ast;
use syntax::attr;
use syntax::ast::{Ident, NodeId};
use syntax::ptr::P;

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
    pub vis: ast::Visibility,
    pub stab: Option<attr::Stability>,
    pub impls: Vec<Impl>,
    pub foreigns: Vec<ast::ForeignMod>,
    pub macros: Vec<Macro>,
    pub is_crate: bool,
}

impl Module {
    pub fn new(name: Option<Ident>) -> Module {
        Module {
            name       : name,
            id: 0,
            vis: ast::Inherited,
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
            foreigns   : Vec::new(),
            macros     : Vec::new(),
            is_crate   : false,
        }
    }
}

#[derive(Show, Clone, RustcEncodable, RustcDecodable, Copy)]
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
    TraitBound(ast::TraitRef)
}

pub struct Struct {
    pub vis: ast::Visibility,
    pub stab: Option<attr::Stability>,
    pub id: NodeId,
    pub struct_type: StructType,
    pub name: Ident,
    pub generics: ast::Generics,
    pub attrs: Vec<ast::Attribute>,
    pub fields: Vec<ast::StructField>,
    pub whence: Span,
}

pub struct Enum {
    pub vis: ast::Visibility,
    pub stab: Option<attr::Stability>,
    pub variants: Vec<Variant>,
    pub generics: ast::Generics,
    pub attrs: Vec<ast::Attribute>,
    pub id: NodeId,
    pub whence: Span,
    pub name: Ident,
}

pub struct Variant {
    pub name: Ident,
    pub attrs: Vec<ast::Attribute>,
    pub kind: ast::VariantKind,
    pub id: ast::NodeId,
    pub vis: ast::Visibility,
    pub stab: Option<attr::Stability>,
    pub whence: Span,
}

pub struct Function {
    pub decl: ast::FnDecl,
    pub attrs: Vec<ast::Attribute>,
    pub id: NodeId,
    pub name: Ident,
    pub vis: ast::Visibility,
    pub stab: Option<attr::Stability>,
    pub unsafety: ast::Unsafety,
    pub whence: Span,
    pub generics: ast::Generics,
}

pub struct Typedef {
    pub ty: P<ast::Ty>,
    pub gen: ast::Generics,
    pub name: Ident,
    pub id: ast::NodeId,
    pub attrs: Vec<ast::Attribute>,
    pub whence: Span,
    pub vis: ast::Visibility,
    pub stab: Option<attr::Stability>,
}

#[derive(Show)]
pub struct Static {
    pub type_: P<ast::Ty>,
    pub mutability: ast::Mutability,
    pub expr: P<ast::Expr>,
    pub name: Ident,
    pub attrs: Vec<ast::Attribute>,
    pub vis: ast::Visibility,
    pub stab: Option<attr::Stability>,
    pub id: ast::NodeId,
    pub whence: Span,
}

pub struct Constant {
    pub type_: P<ast::Ty>,
    pub expr: P<ast::Expr>,
    pub name: Ident,
    pub attrs: Vec<ast::Attribute>,
    pub vis: ast::Visibility,
    pub stab: Option<attr::Stability>,
    pub id: ast::NodeId,
    pub whence: Span,
}

pub struct Trait {
    pub unsafety: ast::Unsafety,
    pub name: Ident,
    pub items: Vec<ast::TraitItem>, //should be TraitItem
    pub generics: ast::Generics,
    pub bounds: Vec<ast::TyParamBound>,
    pub attrs: Vec<ast::Attribute>,
    pub id: ast::NodeId,
    pub whence: Span,
    pub vis: ast::Visibility,
    pub stab: Option<attr::Stability>,
}

pub struct Impl {
    pub unsafety: ast::Unsafety,
    pub polarity: ast::ImplPolarity,
    pub generics: ast::Generics,
    pub trait_: Option<ast::TraitRef>,
    pub for_: P<ast::Ty>,
    pub items: Vec<ast::ImplItem>,
    pub attrs: Vec<ast::Attribute>,
    pub whence: Span,
    pub vis: ast::Visibility,
    pub stab: Option<attr::Stability>,
    pub id: ast::NodeId,
}

pub struct Macro {
    pub name: Ident,
    pub id: ast::NodeId,
    pub attrs: Vec<ast::Attribute>,
    pub whence: Span,
    pub stab: Option<attr::Stability>,
}

pub struct ExternCrate {
    pub name: Ident,
    pub path: Option<String>,
    pub vis: ast::Visibility,
    pub attrs: Vec<ast::Attribute>,
    pub whence: Span,
}

pub struct Import {
    pub id: NodeId,
    pub vis: ast::Visibility,
    pub attrs: Vec<ast::Attribute>,
    pub node: ast::ViewPath_,
    pub whence: Span,
}

pub fn struct_type_from_def(sd: &ast::StructDef) -> StructType {
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
