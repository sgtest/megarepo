// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!
A mini version of ast::Ty, which is easier to use, and features an
explicit `Self` type to use when specifying impls to be derived.
*/

use ast;
use ast::{P,Expr,Generics,Ident};
use ext::base::ExtCtxt;
use ext::build::AstBuilder;
use codemap::{Span,respan};
use owned_slice::OwnedSlice;

use std::gc::Gc;

/// The types of pointers
pub enum PtrTy<'a> {
    Send, // ~
    Borrowed(Option<&'a str>, ast::Mutability), // &['lifetime] [mut]
}

/// A path, e.g. `::std::option::Option::<int>` (global). Has support
/// for type parameters and a lifetime.
pub struct Path<'a> {
    pub path: Vec<&'a str> ,
    pub lifetime: Option<&'a str>,
    pub params: Vec<Box<Ty<'a>>>,
    pub global: bool,
}

impl<'a> Path<'a> {
    pub fn new<'r>(path: Vec<&'r str> ) -> Path<'r> {
        Path::new_(path, None, Vec::new(), true)
    }
    pub fn new_local<'r>(path: &'r str) -> Path<'r> {
        Path::new_(vec!( path ), None, Vec::new(), false)
    }
    pub fn new_<'r>(path: Vec<&'r str> ,
                    lifetime: Option<&'r str>,
                    params: Vec<Box<Ty<'r>>>,
                    global: bool)
                    -> Path<'r> {
        Path {
            path: path,
            lifetime: lifetime,
            params: params,
            global: global
        }
    }

    pub fn to_ty(&self,
                 cx: &ExtCtxt,
                 span: Span,
                 self_ty: Ident,
                 self_generics: &Generics)
                 -> P<ast::Ty> {
        cx.ty_path(self.to_path(cx, span, self_ty, self_generics), None)
    }
    pub fn to_path(&self,
                   cx: &ExtCtxt,
                   span: Span,
                   self_ty: Ident,
                   self_generics: &Generics)
                   -> ast::Path {
        let idents = self.path.iter().map(|s| cx.ident_of(*s)).collect();
        let lt = mk_lifetimes(cx, span, &self.lifetime);
        let tys = self.params.iter().map(|t| t.to_ty(cx, span, self_ty, self_generics)).collect();

        cx.path_all(span, self.global, idents, lt, tys)
    }
}

/// A type. Supports pointers (except for *), Self, and literals
pub enum Ty<'a> {
    Self,
    // &/Box/ Ty
    Ptr(Box<Ty<'a>>, PtrTy<'a>),
    // mod::mod::Type<[lifetime], [Params...]>, including a plain type
    // parameter, and things like `int`
    Literal(Path<'a>),
    // includes nil
    Tuple(Vec<Ty<'a>> )
}

pub fn borrowed_ptrty<'r>() -> PtrTy<'r> {
    Borrowed(None, ast::MutImmutable)
}
pub fn borrowed<'r>(ty: Box<Ty<'r>>) -> Ty<'r> {
    Ptr(ty, borrowed_ptrty())
}

pub fn borrowed_explicit_self<'r>() -> Option<Option<PtrTy<'r>>> {
    Some(Some(borrowed_ptrty()))
}

pub fn borrowed_self<'r>() -> Ty<'r> {
    borrowed(box Self)
}

pub fn nil_ty() -> Ty<'static> {
    Tuple(Vec::new())
}

fn mk_lifetime(cx: &ExtCtxt, span: Span, lt: &Option<&str>) -> Option<ast::Lifetime> {
    match *lt {
        Some(ref s) => Some(cx.lifetime(span, cx.ident_of(*s).name)),
        None => None
    }
}

fn mk_lifetimes(cx: &ExtCtxt, span: Span, lt: &Option<&str>) -> Vec<ast::Lifetime> {
    match *lt {
        Some(ref s) => vec!(cx.lifetime(span, cx.ident_of(*s).name)),
        None => vec!()
    }
}

impl<'a> Ty<'a> {
    pub fn to_ty(&self,
                 cx: &ExtCtxt,
                 span: Span,
                 self_ty: Ident,
                 self_generics: &Generics)
                 -> P<ast::Ty> {
        match *self {
            Ptr(ref ty, ref ptr) => {
                let raw_ty = ty.to_ty(cx, span, self_ty, self_generics);
                match *ptr {
                    Send => {
                        cx.ty_uniq(span, raw_ty)
                    }
                    Borrowed(ref lt, mutbl) => {
                        let lt = mk_lifetime(cx, span, lt);
                        cx.ty_rptr(span, raw_ty, lt, mutbl)
                    }
                }
            }
            Literal(ref p) => { p.to_ty(cx, span, self_ty, self_generics) }
            Self  => {
                cx.ty_path(self.to_path(cx, span, self_ty, self_generics), None)
            }
            Tuple(ref fields) => {
                let ty = if fields.is_empty() {
                    ast::TyNil
                } else {
                    ast::TyTup(fields.iter()
                                     .map(|f| f.to_ty(cx, span, self_ty, self_generics))
                                     .collect())
                };

                cx.ty(span, ty)
            }
        }
    }

    pub fn to_path(&self,
                   cx: &ExtCtxt,
                   span: Span,
                   self_ty: Ident,
                   self_generics: &Generics)
                   -> ast::Path {
        match *self {
            Self => {
                let self_params = self_generics.ty_params.map(|ty_param| {
                    cx.ty_ident(span, ty_param.ident)
                });
                let lifetimes = self_generics.lifetimes.clone();

                cx.path_all(span, false, vec!(self_ty), lifetimes,
                            self_params.into_vec())
            }
            Literal(ref p) => {
                p.to_path(cx, span, self_ty, self_generics)
            }
            Ptr(..) => { cx.span_bug(span, "pointer in a path in generic `deriving`") }
            Tuple(..) => { cx.span_bug(span, "tuple in a path in generic `deriving`") }
        }
    }
}


fn mk_ty_param(cx: &ExtCtxt, span: Span, name: &str, sized: ast::Sized, bounds: &[Path],
               self_ident: Ident, self_generics: &Generics) -> ast::TyParam {
    let bounds =
        bounds.iter().map(|b| {
            let path = b.to_path(cx, span, self_ident, self_generics);
            cx.typarambound(path)
        }).collect();
    cx.typaram(span, cx.ident_of(name), sized, bounds, None)
}

fn mk_generics(lifetimes: Vec<ast::Lifetime> ,  ty_params: Vec<ast::TyParam> ) -> Generics {
    Generics {
        lifetimes: lifetimes,
        ty_params: OwnedSlice::from_vec(ty_params)
    }
}

/// Lifetimes and bounds on type parameters
pub struct LifetimeBounds<'a> {
    pub lifetimes: Vec<&'a str>,
    pub bounds: Vec<(&'a str, ast::Sized, Vec<Path<'a>>)>,
}

impl<'a> LifetimeBounds<'a> {
    pub fn empty() -> LifetimeBounds<'static> {
        LifetimeBounds {
            lifetimes: Vec::new(), bounds: Vec::new()
        }
    }
    pub fn to_generics(&self,
                       cx: &ExtCtxt,
                       span: Span,
                       self_ty: Ident,
                       self_generics: &Generics)
                       -> Generics {
        let lifetimes = self.lifetimes.iter().map(|lt| {
            cx.lifetime(span, cx.ident_of(*lt).name)
        }).collect();
        let ty_params = self.bounds.iter().map(|t| {
            match t {
                &(ref name, sized, ref bounds) => {
                    mk_ty_param(cx,
                                span,
                                *name,
                                sized,
                                bounds.as_slice(),
                                self_ty,
                                self_generics)
                }
            }
        }).collect();
        mk_generics(lifetimes, ty_params)
    }
}


pub fn get_explicit_self(cx: &ExtCtxt, span: Span, self_ptr: &Option<PtrTy>)
    -> (Gc<Expr>, ast::ExplicitSelf) {
    let self_path = cx.expr_self(span);
    match *self_ptr {
        None => {
            (self_path, respan(span, ast::SelfValue))
        }
        Some(ref ptr) => {
            let self_ty = respan(
                span,
                match *ptr {
                    Send => ast::SelfUniq,
                    Borrowed(ref lt, mutbl) => {
                        let lt = lt.map(|s| cx.lifetime(span, cx.ident_of(s).name));
                        ast::SelfRegion(lt, mutbl)
                    }
                });
            let self_expr = cx.expr_deref(span, self_path);
            (self_expr, self_ty)
        }
    }
}
