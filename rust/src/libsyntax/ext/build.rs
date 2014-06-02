// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use abi;
use ast::{P, Ident, Generics, NodeId, Expr};
use ast;
use ast_util;
use attr;
use codemap::{Span, respan, Spanned, DUMMY_SP};
use ext::base::ExtCtxt;
use fold::Folder;
use owned_slice::OwnedSlice;
use parse::token::special_idents;
use parse::token::InternedString;
use parse::token;

pub struct Field {
    ident: ast::Ident,
    ex: @ast::Expr
}

// Transitional reexports so qquote can find the paths it is looking for
mod syntax {
    pub use ext;
    pub use parse;
}

pub trait AstBuilder {
    // paths
    fn path(&self, span: Span, strs: Vec<ast::Ident> ) -> ast::Path;
    fn path_ident(&self, span: Span, id: ast::Ident) -> ast::Path;
    fn path_global(&self, span: Span, strs: Vec<ast::Ident> ) -> ast::Path;
    fn path_all(&self, sp: Span,
                global: bool,
                idents: Vec<ast::Ident> ,
                lifetimes: Vec<ast::Lifetime>,
                types: Vec<P<ast::Ty>> )
        -> ast::Path;

    // types
    fn ty_mt(&self, ty: P<ast::Ty>, mutbl: ast::Mutability) -> ast::MutTy;

    fn ty(&self, span: Span, ty: ast::Ty_) -> P<ast::Ty>;
    fn ty_path(&self, ast::Path, Option<OwnedSlice<ast::TyParamBound>>) -> P<ast::Ty>;
    fn ty_ident(&self, span: Span, idents: ast::Ident) -> P<ast::Ty>;

    fn ty_rptr(&self, span: Span,
               ty: P<ast::Ty>,
               lifetime: Option<ast::Lifetime>,
               mutbl: ast::Mutability) -> P<ast::Ty>;
    fn ty_uniq(&self, span: Span, ty: P<ast::Ty>) -> P<ast::Ty>;

    fn ty_option(&self, ty: P<ast::Ty>) -> P<ast::Ty>;
    fn ty_infer(&self, sp: Span) -> P<ast::Ty>;
    fn ty_nil(&self) -> P<ast::Ty>;

    fn ty_vars(&self, ty_params: &OwnedSlice<ast::TyParam>) -> Vec<P<ast::Ty>> ;
    fn ty_vars_global(&self, ty_params: &OwnedSlice<ast::TyParam>) -> Vec<P<ast::Ty>> ;
    fn ty_field_imm(&self, span: Span, name: Ident, ty: P<ast::Ty>) -> ast::TypeField;
    fn strip_bounds(&self, bounds: &Generics) -> Generics;

    fn typaram(&self,
               span: Span,
               id: ast::Ident,
               sized: ast::Sized,
               bounds: OwnedSlice<ast::TyParamBound>,
               default: Option<P<ast::Ty>>) -> ast::TyParam;

    fn trait_ref(&self, path: ast::Path) -> ast::TraitRef;
    fn typarambound(&self, path: ast::Path) -> ast::TyParamBound;
    fn lifetime(&self, span: Span, ident: ast::Name) -> ast::Lifetime;

    // statements
    fn stmt_expr(&self, expr: @ast::Expr) -> @ast::Stmt;
    fn stmt_let(&self, sp: Span, mutbl: bool, ident: ast::Ident, ex: @ast::Expr) -> @ast::Stmt;
    fn stmt_let_typed(&self,
                      sp: Span,
                      mutbl: bool,
                      ident: ast::Ident,
                      typ: P<ast::Ty>,
                      ex: @ast::Expr)
                      -> @ast::Stmt;

    // blocks
    fn block(&self, span: Span, stmts: Vec<@ast::Stmt> , expr: Option<@ast::Expr>) -> P<ast::Block>;
    fn block_expr(&self, expr: @ast::Expr) -> P<ast::Block>;
    fn block_all(&self, span: Span,
                 view_items: Vec<ast::ViewItem> ,
                 stmts: Vec<@ast::Stmt> ,
                 expr: Option<@ast::Expr>) -> P<ast::Block>;

    // expressions
    fn expr(&self, span: Span, node: ast::Expr_) -> @ast::Expr;
    fn expr_path(&self, path: ast::Path) -> @ast::Expr;
    fn expr_ident(&self, span: Span, id: ast::Ident) -> @ast::Expr;

    fn expr_self(&self, span: Span) -> @ast::Expr;
    fn expr_binary(&self, sp: Span, op: ast::BinOp,
                   lhs: @ast::Expr, rhs: @ast::Expr) -> @ast::Expr;
    fn expr_deref(&self, sp: Span, e: @ast::Expr) -> @ast::Expr;
    fn expr_unary(&self, sp: Span, op: ast::UnOp, e: @ast::Expr) -> @ast::Expr;

    fn expr_managed(&self, sp: Span, e: @ast::Expr) -> @ast::Expr;
    fn expr_addr_of(&self, sp: Span, e: @ast::Expr) -> @ast::Expr;
    fn expr_mut_addr_of(&self, sp: Span, e: @ast::Expr) -> @ast::Expr;
    fn expr_field_access(&self, span: Span, expr: @ast::Expr, ident: ast::Ident) -> @ast::Expr;
    fn expr_call(&self, span: Span, expr: @ast::Expr, args: Vec<@ast::Expr> ) -> @ast::Expr;
    fn expr_call_ident(&self, span: Span, id: ast::Ident, args: Vec<@ast::Expr> ) -> @ast::Expr;
    fn expr_call_global(&self, sp: Span, fn_path: Vec<ast::Ident> ,
                        args: Vec<@ast::Expr> ) -> @ast::Expr;
    fn expr_method_call(&self, span: Span,
                        expr: @ast::Expr, ident: ast::Ident,
                        args: Vec<@ast::Expr> ) -> @ast::Expr;
    fn expr_block(&self, b: P<ast::Block>) -> @ast::Expr;
    fn expr_cast(&self, sp: Span, expr: @ast::Expr, ty: P<ast::Ty>) -> @ast::Expr;

    fn field_imm(&self, span: Span, name: Ident, e: @ast::Expr) -> ast::Field;
    fn expr_struct(&self, span: Span, path: ast::Path, fields: Vec<ast::Field> ) -> @ast::Expr;
    fn expr_struct_ident(&self, span: Span, id: ast::Ident, fields: Vec<ast::Field> ) -> @ast::Expr;

    fn expr_lit(&self, sp: Span, lit: ast::Lit_) -> @ast::Expr;

    fn expr_uint(&self, span: Span, i: uint) -> @ast::Expr;
    fn expr_int(&self, sp: Span, i: int) -> @ast::Expr;
    fn expr_u8(&self, sp: Span, u: u8) -> @ast::Expr;
    fn expr_bool(&self, sp: Span, value: bool) -> @ast::Expr;

    fn expr_vstore(&self, sp: Span, expr: @ast::Expr, vst: ast::ExprVstore) -> @ast::Expr;
    fn expr_vec(&self, sp: Span, exprs: Vec<@ast::Expr> ) -> @ast::Expr;
    fn expr_vec_ng(&self, sp: Span) -> @ast::Expr;
    fn expr_vec_slice(&self, sp: Span, exprs: Vec<@ast::Expr> ) -> @ast::Expr;
    fn expr_str(&self, sp: Span, s: InternedString) -> @ast::Expr;
    fn expr_str_uniq(&self, sp: Span, s: InternedString) -> @ast::Expr;

    fn expr_some(&self, sp: Span, expr: @ast::Expr) -> @ast::Expr;
    fn expr_none(&self, sp: Span) -> @ast::Expr;

    fn expr_fail(&self, span: Span, msg: InternedString) -> @ast::Expr;
    fn expr_unreachable(&self, span: Span) -> @ast::Expr;

    fn expr_ok(&self, span: Span, expr: @ast::Expr) -> @ast::Expr;
    fn expr_err(&self, span: Span, expr: @ast::Expr) -> @ast::Expr;
    fn expr_try(&self, span: Span, head: @ast::Expr) -> @ast::Expr;

    fn pat(&self, span: Span, pat: ast::Pat_) -> @ast::Pat;
    fn pat_wild(&self, span: Span) -> @ast::Pat;
    fn pat_lit(&self, span: Span, expr: @ast::Expr) -> @ast::Pat;
    fn pat_ident(&self, span: Span, ident: ast::Ident) -> @ast::Pat;

    fn pat_ident_binding_mode(&self,
                              span: Span,
                              ident: ast::Ident,
                              bm: ast::BindingMode) -> @ast::Pat;
    fn pat_enum(&self, span: Span, path: ast::Path, subpats: Vec<@ast::Pat> ) -> @ast::Pat;
    fn pat_struct(&self, span: Span,
                  path: ast::Path, field_pats: Vec<ast::FieldPat> ) -> @ast::Pat;

    fn arm(&self, span: Span, pats: Vec<@ast::Pat> , expr: @ast::Expr) -> ast::Arm;
    fn arm_unreachable(&self, span: Span) -> ast::Arm;

    fn expr_match(&self, span: Span, arg: @ast::Expr, arms: Vec<ast::Arm> ) -> @ast::Expr;
    fn expr_if(&self, span: Span,
               cond: @ast::Expr, then: @ast::Expr, els: Option<@ast::Expr>) -> @ast::Expr;

    fn lambda_fn_decl(&self, span: Span,
                      fn_decl: P<ast::FnDecl>, blk: P<ast::Block>) -> @ast::Expr;

    fn lambda(&self, span: Span, ids: Vec<ast::Ident> , blk: P<ast::Block>) -> @ast::Expr;
    fn lambda0(&self, span: Span, blk: P<ast::Block>) -> @ast::Expr;
    fn lambda1(&self, span: Span, blk: P<ast::Block>, ident: ast::Ident) -> @ast::Expr;

    fn lambda_expr(&self, span: Span, ids: Vec<ast::Ident> , blk: @ast::Expr) -> @ast::Expr;
    fn lambda_expr_0(&self, span: Span, expr: @ast::Expr) -> @ast::Expr;
    fn lambda_expr_1(&self, span: Span, expr: @ast::Expr, ident: ast::Ident) -> @ast::Expr;

    fn lambda_stmts(&self, span: Span, ids: Vec<ast::Ident> , blk: Vec<@ast::Stmt> ) -> @ast::Expr;
    fn lambda_stmts_0(&self, span: Span, stmts: Vec<@ast::Stmt> ) -> @ast::Expr;
    fn lambda_stmts_1(&self, span: Span, stmts: Vec<@ast::Stmt> , ident: ast::Ident) -> @ast::Expr;

    // items
    fn item(&self, span: Span,
            name: Ident, attrs: Vec<ast::Attribute> , node: ast::Item_) -> @ast::Item;

    fn arg(&self, span: Span, name: Ident, ty: P<ast::Ty>) -> ast::Arg;
    // FIXME unused self
    fn fn_decl(&self, inputs: Vec<ast::Arg> , output: P<ast::Ty>) -> P<ast::FnDecl>;

    fn item_fn_poly(&self,
                    span: Span,
                    name: Ident,
                    inputs: Vec<ast::Arg> ,
                    output: P<ast::Ty>,
                    generics: Generics,
                    body: P<ast::Block>) -> @ast::Item;
    fn item_fn(&self,
               span: Span,
               name: Ident,
               inputs: Vec<ast::Arg> ,
               output: P<ast::Ty>,
               body: P<ast::Block>) -> @ast::Item;

    fn variant(&self, span: Span, name: Ident, tys: Vec<P<ast::Ty>> ) -> ast::Variant;
    fn item_enum_poly(&self,
                      span: Span,
                      name: Ident,
                      enum_definition: ast::EnumDef,
                      generics: Generics) -> @ast::Item;
    fn item_enum(&self, span: Span, name: Ident, enum_def: ast::EnumDef) -> @ast::Item;

    fn item_struct_poly(&self,
                        span: Span,
                        name: Ident,
                        struct_def: ast::StructDef,
                        generics: Generics) -> @ast::Item;
    fn item_struct(&self, span: Span, name: Ident, struct_def: ast::StructDef) -> @ast::Item;

    fn item_mod(&self, span: Span, inner_span: Span,
                name: Ident, attrs: Vec<ast::Attribute> ,
                vi: Vec<ast::ViewItem> , items: Vec<@ast::Item> ) -> @ast::Item;

    fn item_ty_poly(&self,
                    span: Span,
                    name: Ident,
                    ty: P<ast::Ty>,
                    generics: Generics) -> @ast::Item;
    fn item_ty(&self, span: Span, name: Ident, ty: P<ast::Ty>) -> @ast::Item;

    fn attribute(&self, sp: Span, mi: @ast::MetaItem) -> ast::Attribute;

    fn meta_word(&self, sp: Span, w: InternedString) -> @ast::MetaItem;
    fn meta_list(&self,
                 sp: Span,
                 name: InternedString,
                 mis: Vec<@ast::MetaItem> )
                 -> @ast::MetaItem;
    fn meta_name_value(&self,
                       sp: Span,
                       name: InternedString,
                       value: ast::Lit_)
                       -> @ast::MetaItem;

    fn view_use(&self, sp: Span,
                vis: ast::Visibility, vp: @ast::ViewPath) -> ast::ViewItem;
    fn view_use_simple(&self, sp: Span, vis: ast::Visibility, path: ast::Path) -> ast::ViewItem;
    fn view_use_simple_(&self, sp: Span, vis: ast::Visibility,
                        ident: ast::Ident, path: ast::Path) -> ast::ViewItem;
    fn view_use_list(&self, sp: Span, vis: ast::Visibility,
                     path: Vec<ast::Ident> , imports: &[ast::Ident]) -> ast::ViewItem;
    fn view_use_glob(&self, sp: Span,
                     vis: ast::Visibility, path: Vec<ast::Ident> ) -> ast::ViewItem;
}

impl<'a> AstBuilder for ExtCtxt<'a> {
    fn path(&self, span: Span, strs: Vec<ast::Ident> ) -> ast::Path {
        self.path_all(span, false, strs, Vec::new(), Vec::new())
    }
    fn path_ident(&self, span: Span, id: ast::Ident) -> ast::Path {
        self.path(span, vec!(id))
    }
    fn path_global(&self, span: Span, strs: Vec<ast::Ident> ) -> ast::Path {
        self.path_all(span, true, strs, Vec::new(), Vec::new())
    }
    fn path_all(&self,
                sp: Span,
                global: bool,
                mut idents: Vec<ast::Ident> ,
                lifetimes: Vec<ast::Lifetime>,
                types: Vec<P<ast::Ty>> )
                -> ast::Path {
        let last_identifier = idents.pop().unwrap();
        let mut segments: Vec<ast::PathSegment> = idents.move_iter()
                                                      .map(|ident| {
            ast::PathSegment {
                identifier: ident,
                lifetimes: Vec::new(),
                types: OwnedSlice::empty(),
            }
        }).collect();
        segments.push(ast::PathSegment {
            identifier: last_identifier,
            lifetimes: lifetimes,
            types: OwnedSlice::from_vec(types),
        });
        ast::Path {
            span: sp,
            global: global,
            segments: segments,
        }
    }

    fn ty_mt(&self, ty: P<ast::Ty>, mutbl: ast::Mutability) -> ast::MutTy {
        ast::MutTy {
            ty: ty,
            mutbl: mutbl
        }
    }

    fn ty(&self, span: Span, ty: ast::Ty_) -> P<ast::Ty> {
        P(ast::Ty {
            id: ast::DUMMY_NODE_ID,
            span: span,
            node: ty
        })
    }

    fn ty_path(&self, path: ast::Path, bounds: Option<OwnedSlice<ast::TyParamBound>>)
              -> P<ast::Ty> {
        self.ty(path.span,
                ast::TyPath(path, bounds, ast::DUMMY_NODE_ID))
    }

    // Might need to take bounds as an argument in the future, if you ever want
    // to generate a bounded existential trait type.
    fn ty_ident(&self, span: Span, ident: ast::Ident)
        -> P<ast::Ty> {
        self.ty_path(self.path_ident(span, ident), None)
    }

    fn ty_rptr(&self,
               span: Span,
               ty: P<ast::Ty>,
               lifetime: Option<ast::Lifetime>,
               mutbl: ast::Mutability)
        -> P<ast::Ty> {
        self.ty(span,
                ast::TyRptr(lifetime, self.ty_mt(ty, mutbl)))
    }

    fn ty_uniq(&self, span: Span, ty: P<ast::Ty>) -> P<ast::Ty> {
        self.ty(span, ast::TyUniq(ty))
    }

    fn ty_option(&self, ty: P<ast::Ty>) -> P<ast::Ty> {
        self.ty_path(
            self.path_all(DUMMY_SP,
                          true,
                          vec!(
                              self.ident_of("std"),
                              self.ident_of("option"),
                              self.ident_of("Option")
                          ),
                          Vec::new(),
                          vec!( ty )), None)
    }

    fn ty_field_imm(&self, span: Span, name: Ident, ty: P<ast::Ty>) -> ast::TypeField {
        ast::TypeField {
            ident: name,
            mt: ast::MutTy { ty: ty, mutbl: ast::MutImmutable },
            span: span,
        }
    }

    fn ty_infer(&self, span: Span) -> P<ast::Ty> {
        self.ty(span, ast::TyInfer)
    }

    fn ty_nil(&self) -> P<ast::Ty> {
        P(ast::Ty {
            id: ast::DUMMY_NODE_ID,
            node: ast::TyNil,
            span: DUMMY_SP,
        })
    }

    fn typaram(&self,
               span: Span,
               id: ast::Ident,
               sized: ast::Sized,
               bounds: OwnedSlice<ast::TyParamBound>,
               default: Option<P<ast::Ty>>) -> ast::TyParam {
        ast::TyParam {
            ident: id,
            id: ast::DUMMY_NODE_ID,
            sized: sized,
            bounds: bounds,
            default: default,
            span: span
        }
    }

    // these are strange, and probably shouldn't be used outside of
    // pipes. Specifically, the global version possible generates
    // incorrect code.
    fn ty_vars(&self, ty_params: &OwnedSlice<ast::TyParam>) -> Vec<P<ast::Ty>> {
        ty_params.iter().map(|p| self.ty_ident(DUMMY_SP, p.ident)).collect()
    }

    fn ty_vars_global(&self, ty_params: &OwnedSlice<ast::TyParam>) -> Vec<P<ast::Ty>> {
        ty_params.iter().map(|p| self.ty_path(
                self.path_global(DUMMY_SP, vec!(p.ident)), None)).collect()
    }

    fn strip_bounds(&self, generics: &Generics) -> Generics {
        let new_params = generics.ty_params.map(|ty_param| {
            ast::TyParam { bounds: OwnedSlice::empty(), ..*ty_param }
        });
        Generics {
            ty_params: new_params,
            .. (*generics).clone()
        }
    }

    fn trait_ref(&self, path: ast::Path) -> ast::TraitRef {
        ast::TraitRef {
            path: path,
            ref_id: ast::DUMMY_NODE_ID
        }
    }

    fn typarambound(&self, path: ast::Path) -> ast::TyParamBound {
        ast::TraitTyParamBound(self.trait_ref(path))
    }

    fn lifetime(&self, span: Span, name: ast::Name) -> ast::Lifetime {
        ast::Lifetime { id: ast::DUMMY_NODE_ID, span: span, name: name }
    }

    fn stmt_expr(&self, expr: @ast::Expr) -> @ast::Stmt {
        @respan(expr.span, ast::StmtSemi(expr, ast::DUMMY_NODE_ID))
    }

    fn stmt_let(&self, sp: Span, mutbl: bool, ident: ast::Ident, ex: @ast::Expr) -> @ast::Stmt {
        let pat = if mutbl {
            self.pat_ident_binding_mode(sp, ident, ast::BindByValue(ast::MutMutable))
        } else {
            self.pat_ident(sp, ident)
        };
        let local = @ast::Local {
            ty: self.ty_infer(sp),
            pat: pat,
            init: Some(ex),
            id: ast::DUMMY_NODE_ID,
            span: sp,
            source: ast::LocalLet,
        };
        let decl = respan(sp, ast::DeclLocal(local));
        @respan(sp, ast::StmtDecl(@decl, ast::DUMMY_NODE_ID))
    }

    fn stmt_let_typed(&self,
                      sp: Span,
                      mutbl: bool,
                      ident: ast::Ident,
                      typ: P<ast::Ty>,
                      ex: @ast::Expr)
                      -> @ast::Stmt {
        let pat = if mutbl {
            self.pat_ident_binding_mode(sp, ident, ast::BindByValue(ast::MutMutable))
        } else {
            self.pat_ident(sp, ident)
        };
        let local = @ast::Local {
            ty: typ,
            pat: pat,
            init: Some(ex),
            id: ast::DUMMY_NODE_ID,
            span: sp,
            source: ast::LocalLet,
        };
        let decl = respan(sp, ast::DeclLocal(local));
        @respan(sp, ast::StmtDecl(@decl, ast::DUMMY_NODE_ID))
    }

    fn block(&self, span: Span, stmts: Vec<@ast::Stmt> , expr: Option<@Expr>) -> P<ast::Block> {
        self.block_all(span, Vec::new(), stmts, expr)
    }

    fn block_expr(&self, expr: @ast::Expr) -> P<ast::Block> {
        self.block_all(expr.span, Vec::new(), Vec::new(), Some(expr))
    }
    fn block_all(&self,
                 span: Span,
                 view_items: Vec<ast::ViewItem> ,
                 stmts: Vec<@ast::Stmt> ,
                 expr: Option<@ast::Expr>) -> P<ast::Block> {
            P(ast::Block {
               view_items: view_items,
               stmts: stmts,
               expr: expr,
               id: ast::DUMMY_NODE_ID,
               rules: ast::DefaultBlock,
               span: span,
            })
    }

    fn expr(&self, span: Span, node: ast::Expr_) -> @ast::Expr {
        @ast::Expr {
            id: ast::DUMMY_NODE_ID,
            node: node,
            span: span,
        }
    }

    fn expr_path(&self, path: ast::Path) -> @ast::Expr {
        self.expr(path.span, ast::ExprPath(path))
    }

    fn expr_ident(&self, span: Span, id: ast::Ident) -> @ast::Expr {
        self.expr_path(self.path_ident(span, id))
    }
    fn expr_self(&self, span: Span) -> @ast::Expr {
        self.expr_ident(span, special_idents::self_)
    }

    fn expr_binary(&self, sp: Span, op: ast::BinOp,
                   lhs: @ast::Expr, rhs: @ast::Expr) -> @ast::Expr {
        self.expr(sp, ast::ExprBinary(op, lhs, rhs))
    }

    fn expr_deref(&self, sp: Span, e: @ast::Expr) -> @ast::Expr {
        self.expr_unary(sp, ast::UnDeref, e)
    }
    fn expr_unary(&self, sp: Span, op: ast::UnOp, e: @ast::Expr) -> @ast::Expr {
        self.expr(sp, ast::ExprUnary(op, e))
    }

    fn expr_managed(&self, sp: Span, e: @ast::Expr) -> @ast::Expr {
        self.expr_unary(sp, ast::UnBox, e)
    }

    fn expr_field_access(&self, sp: Span, expr: @ast::Expr, ident: ast::Ident) -> @ast::Expr {
        self.expr(sp, ast::ExprField(expr, ident, Vec::new()))
    }
    fn expr_addr_of(&self, sp: Span, e: @ast::Expr) -> @ast::Expr {
        self.expr(sp, ast::ExprAddrOf(ast::MutImmutable, e))
    }
    fn expr_mut_addr_of(&self, sp: Span, e: @ast::Expr) -> @ast::Expr {
        self.expr(sp, ast::ExprAddrOf(ast::MutMutable, e))
    }

    fn expr_call(&self, span: Span, expr: @ast::Expr, args: Vec<@ast::Expr> ) -> @ast::Expr {
        self.expr(span, ast::ExprCall(expr, args))
    }
    fn expr_call_ident(&self, span: Span, id: ast::Ident, args: Vec<@ast::Expr> ) -> @ast::Expr {
        self.expr(span, ast::ExprCall(self.expr_ident(span, id), args))
    }
    fn expr_call_global(&self, sp: Span, fn_path: Vec<ast::Ident> ,
                      args: Vec<@ast::Expr> ) -> @ast::Expr {
        let pathexpr = self.expr_path(self.path_global(sp, fn_path));
        self.expr_call(sp, pathexpr, args)
    }
    fn expr_method_call(&self, span: Span,
                        expr: @ast::Expr,
                        ident: ast::Ident,
                        mut args: Vec<@ast::Expr> ) -> @ast::Expr {
        let id = Spanned { node: ident, span: span };
        args.unshift(expr);
        self.expr(span, ast::ExprMethodCall(id, Vec::new(), args))
    }
    fn expr_block(&self, b: P<ast::Block>) -> @ast::Expr {
        self.expr(b.span, ast::ExprBlock(b))
    }
    fn field_imm(&self, span: Span, name: Ident, e: @ast::Expr) -> ast::Field {
        ast::Field { ident: respan(span, name), expr: e, span: span }
    }
    fn expr_struct(&self, span: Span, path: ast::Path, fields: Vec<ast::Field> ) -> @ast::Expr {
        self.expr(span, ast::ExprStruct(path, fields, None))
    }
    fn expr_struct_ident(&self, span: Span,
                         id: ast::Ident, fields: Vec<ast::Field> ) -> @ast::Expr {
        self.expr_struct(span, self.path_ident(span, id), fields)
    }

    fn expr_lit(&self, sp: Span, lit: ast::Lit_) -> @ast::Expr {
        self.expr(sp, ast::ExprLit(@respan(sp, lit)))
    }
    fn expr_uint(&self, span: Span, i: uint) -> @ast::Expr {
        self.expr_lit(span, ast::LitUint(i as u64, ast::TyU))
    }
    fn expr_int(&self, sp: Span, i: int) -> @ast::Expr {
        self.expr_lit(sp, ast::LitInt(i as i64, ast::TyI))
    }
    fn expr_u8(&self, sp: Span, u: u8) -> @ast::Expr {
        self.expr_lit(sp, ast::LitUint(u as u64, ast::TyU8))
    }
    fn expr_bool(&self, sp: Span, value: bool) -> @ast::Expr {
        self.expr_lit(sp, ast::LitBool(value))
    }

    fn expr_vstore(&self, sp: Span, expr: @ast::Expr, vst: ast::ExprVstore) -> @ast::Expr {
        self.expr(sp, ast::ExprVstore(expr, vst))
    }
    fn expr_vec(&self, sp: Span, exprs: Vec<@ast::Expr> ) -> @ast::Expr {
        self.expr(sp, ast::ExprVec(exprs))
    }
    fn expr_vec_ng(&self, sp: Span) -> @ast::Expr {
        self.expr_call_global(sp,
                              vec!(self.ident_of("std"),
                                   self.ident_of("vec"),
                                   self.ident_of("Vec"),
                                   self.ident_of("new")),
                              Vec::new())
    }
    fn expr_vec_slice(&self, sp: Span, exprs: Vec<@ast::Expr> ) -> @ast::Expr {
        self.expr_vstore(sp, self.expr_vec(sp, exprs), ast::ExprVstoreSlice)
    }
    fn expr_str(&self, sp: Span, s: InternedString) -> @ast::Expr {
        self.expr_lit(sp, ast::LitStr(s, ast::CookedStr))
    }
    fn expr_str_uniq(&self, sp: Span, s: InternedString) -> @ast::Expr {
        self.expr_vstore(sp, self.expr_str(sp, s), ast::ExprVstoreUniq)
    }


    fn expr_cast(&self, sp: Span, expr: @ast::Expr, ty: P<ast::Ty>) -> @ast::Expr {
        self.expr(sp, ast::ExprCast(expr, ty))
    }


    fn expr_some(&self, sp: Span, expr: @ast::Expr) -> @ast::Expr {
        let some = vec!(
            self.ident_of("std"),
            self.ident_of("option"),
            self.ident_of("Some"));
        self.expr_call_global(sp, some, vec!(expr))
    }

    fn expr_none(&self, sp: Span) -> @ast::Expr {
        let none = self.path_global(sp, vec!(
            self.ident_of("std"),
            self.ident_of("option"),
            self.ident_of("None")));
        self.expr_path(none)
    }

    fn expr_fail(&self, span: Span, msg: InternedString) -> @ast::Expr {
        let loc = self.codemap().lookup_char_pos(span.lo);
        self.expr_call_global(
            span,
            vec!(
                self.ident_of("std"),
                self.ident_of("rt"),
                self.ident_of("begin_unwind")),
            vec!(
                self.expr_str(span, msg),
                self.expr_str(span,
                              token::intern_and_get_ident(loc.file
                                                             .name
                                                             .as_slice())),
                self.expr_uint(span, loc.line)))
    }

    fn expr_unreachable(&self, span: Span) -> @ast::Expr {
        self.expr_fail(span,
                       InternedString::new(
                           "internal error: entered unreachable code"))
    }

    fn expr_ok(&self, sp: Span, expr: @ast::Expr) -> @ast::Expr {
        let ok = vec!(
            self.ident_of("std"),
            self.ident_of("result"),
            self.ident_of("Ok"));
        self.expr_call_global(sp, ok, vec!(expr))
    }

    fn expr_err(&self, sp: Span, expr: @ast::Expr) -> @ast::Expr {
        let err = vec!(
            self.ident_of("std"),
            self.ident_of("result"),
            self.ident_of("Err"));
        self.expr_call_global(sp, err, vec!(expr))
    }

    fn expr_try(&self, sp: Span, head: @ast::Expr) -> @ast::Expr {
        let ok = self.ident_of("Ok");
        let ok_path = self.path_ident(sp, ok);
        let err = self.ident_of("Err");
        let err_path = self.path_ident(sp, err);

        let binding_variable = self.ident_of("__try_var");
        let binding_pat = self.pat_ident(sp, binding_variable);
        let binding_expr = self.expr_ident(sp, binding_variable);

        // Ok(__try_var) pattern
        let ok_pat = self.pat_enum(sp, ok_path, vec!(binding_pat));

        // Err(__try_var)  (pattern and expression resp.)
        let err_pat = self.pat_enum(sp, err_path, vec!(binding_pat));
        let err_inner_expr = self.expr_call_ident(sp, err, vec!(binding_expr));
        // return Err(__try_var)
        let err_expr = self.expr(sp, ast::ExprRet(Some(err_inner_expr)));

        // Ok(__try_var) => __try_var
        let ok_arm = self.arm(sp, vec!(ok_pat), binding_expr);
        // Err(__try_var) => return Err(__try_var)
        let err_arm = self.arm(sp, vec!(err_pat), err_expr);

        // match head { Ok() => ..., Err() => ... }
        self.expr_match(sp, head, vec!(ok_arm, err_arm))
    }


    fn pat(&self, span: Span, pat: ast::Pat_) -> @ast::Pat {
        @ast::Pat { id: ast::DUMMY_NODE_ID, node: pat, span: span }
    }
    fn pat_wild(&self, span: Span) -> @ast::Pat {
        self.pat(span, ast::PatWild)
    }
    fn pat_lit(&self, span: Span, expr: @ast::Expr) -> @ast::Pat {
        self.pat(span, ast::PatLit(expr))
    }
    fn pat_ident(&self, span: Span, ident: ast::Ident) -> @ast::Pat {
        self.pat_ident_binding_mode(span, ident, ast::BindByValue(ast::MutImmutable))
    }

    fn pat_ident_binding_mode(&self,
                              span: Span,
                              ident: ast::Ident,
                              bm: ast::BindingMode) -> @ast::Pat {
        let path = self.path_ident(span, ident);
        let pat = ast::PatIdent(bm, path, None);
        self.pat(span, pat)
    }
    fn pat_enum(&self, span: Span, path: ast::Path, subpats: Vec<@ast::Pat> ) -> @ast::Pat {
        let pat = ast::PatEnum(path, Some(subpats));
        self.pat(span, pat)
    }
    fn pat_struct(&self, span: Span,
                  path: ast::Path, field_pats: Vec<ast::FieldPat> ) -> @ast::Pat {
        let pat = ast::PatStruct(path, field_pats, false);
        self.pat(span, pat)
    }

    fn arm(&self, _span: Span, pats: Vec<@ast::Pat> , expr: @ast::Expr) -> ast::Arm {
        ast::Arm {
            attrs: vec!(),
            pats: pats,
            guard: None,
            body: expr
        }
    }

    fn arm_unreachable(&self, span: Span) -> ast::Arm {
        self.arm(span, vec!(self.pat_wild(span)), self.expr_unreachable(span))
    }

    fn expr_match(&self, span: Span, arg: @ast::Expr, arms: Vec<ast::Arm> ) -> @Expr {
        self.expr(span, ast::ExprMatch(arg, arms))
    }

    fn expr_if(&self, span: Span,
               cond: @ast::Expr, then: @ast::Expr, els: Option<@ast::Expr>) -> @ast::Expr {
        let els = els.map(|x| self.expr_block(self.block_expr(x)));
        self.expr(span, ast::ExprIf(cond, self.block_expr(then), els))
    }

    fn lambda_fn_decl(&self, span: Span,
                      fn_decl: P<ast::FnDecl>, blk: P<ast::Block>) -> @ast::Expr {
        self.expr(span, ast::ExprFnBlock(fn_decl, blk))
    }
    fn lambda(&self, span: Span, ids: Vec<ast::Ident> , blk: P<ast::Block>) -> @ast::Expr {
        let fn_decl = self.fn_decl(
            ids.iter().map(|id| self.arg(span, *id, self.ty_infer(span))).collect(),
            self.ty_infer(span));

        self.expr(span, ast::ExprFnBlock(fn_decl, blk))
    }
    fn lambda0(&self, span: Span, blk: P<ast::Block>) -> @ast::Expr {
        self.lambda(span, Vec::new(), blk)
    }

    fn lambda1(&self, span: Span, blk: P<ast::Block>, ident: ast::Ident) -> @ast::Expr {
        self.lambda(span, vec!(ident), blk)
    }

    fn lambda_expr(&self, span: Span, ids: Vec<ast::Ident> , expr: @ast::Expr) -> @ast::Expr {
        self.lambda(span, ids, self.block_expr(expr))
    }
    fn lambda_expr_0(&self, span: Span, expr: @ast::Expr) -> @ast::Expr {
        self.lambda0(span, self.block_expr(expr))
    }
    fn lambda_expr_1(&self, span: Span, expr: @ast::Expr, ident: ast::Ident) -> @ast::Expr {
        self.lambda1(span, self.block_expr(expr), ident)
    }

    fn lambda_stmts(&self,
                    span: Span,
                    ids: Vec<ast::Ident>,
                    stmts: Vec<@ast::Stmt>)
                    -> @ast::Expr {
        self.lambda(span, ids, self.block(span, stmts, None))
    }
    fn lambda_stmts_0(&self, span: Span, stmts: Vec<@ast::Stmt> ) -> @ast::Expr {
        self.lambda0(span, self.block(span, stmts, None))
    }
    fn lambda_stmts_1(&self, span: Span, stmts: Vec<@ast::Stmt> , ident: ast::Ident) -> @ast::Expr {
        self.lambda1(span, self.block(span, stmts, None), ident)
    }

    fn arg(&self, span: Span, ident: ast::Ident, ty: P<ast::Ty>) -> ast::Arg {
        let arg_pat = self.pat_ident(span, ident);
        ast::Arg {
            ty: ty,
            pat: arg_pat,
            id: ast::DUMMY_NODE_ID
        }
    }

    // FIXME unused self
    fn fn_decl(&self, inputs: Vec<ast::Arg> , output: P<ast::Ty>) -> P<ast::FnDecl> {
        P(ast::FnDecl {
            inputs: inputs,
            output: output,
            cf: ast::Return,
            variadic: false
        })
    }

    fn item(&self, span: Span,
            name: Ident, attrs: Vec<ast::Attribute> , node: ast::Item_) -> @ast::Item {
        // FIXME: Would be nice if our generated code didn't violate
        // Rust coding conventions
        @ast::Item { ident: name,
                    attrs: attrs,
                    id: ast::DUMMY_NODE_ID,
                    node: node,
                    vis: ast::Inherited,
                    span: span }
    }

    fn item_fn_poly(&self,
                    span: Span,
                    name: Ident,
                    inputs: Vec<ast::Arg> ,
                    output: P<ast::Ty>,
                    generics: Generics,
                    body: P<ast::Block>) -> @ast::Item {
        self.item(span,
                  name,
                  Vec::new(),
                  ast::ItemFn(self.fn_decl(inputs, output),
                              ast::NormalFn,
                              abi::Rust,
                              generics,
                              body))
    }

    fn item_fn(&self,
               span: Span,
               name: Ident,
               inputs: Vec<ast::Arg> ,
               output: P<ast::Ty>,
               body: P<ast::Block>
              ) -> @ast::Item {
        self.item_fn_poly(
            span,
            name,
            inputs,
            output,
            ast_util::empty_generics(),
            body)
    }

    fn variant(&self, span: Span, name: Ident, tys: Vec<P<ast::Ty>> ) -> ast::Variant {
        let args = tys.move_iter().map(|ty| {
            ast::VariantArg { ty: ty, id: ast::DUMMY_NODE_ID }
        }).collect();

        respan(span,
               ast::Variant_ {
                   name: name,
                   attrs: Vec::new(),
                   kind: ast::TupleVariantKind(args),
                   id: ast::DUMMY_NODE_ID,
                   disr_expr: None,
                   vis: ast::Public
               })
    }

    fn item_enum_poly(&self, span: Span, name: Ident,
                      enum_definition: ast::EnumDef,
                      generics: Generics) -> @ast::Item {
        self.item(span, name, Vec::new(), ast::ItemEnum(enum_definition, generics))
    }

    fn item_enum(&self, span: Span, name: Ident,
                 enum_definition: ast::EnumDef) -> @ast::Item {
        self.item_enum_poly(span, name, enum_definition,
                            ast_util::empty_generics())
    }

    fn item_struct(&self, span: Span, name: Ident,
                   struct_def: ast::StructDef) -> @ast::Item {
        self.item_struct_poly(
            span,
            name,
            struct_def,
            ast_util::empty_generics()
        )
    }

    fn item_struct_poly(&self, span: Span, name: Ident,
        struct_def: ast::StructDef, generics: Generics) -> @ast::Item {
        self.item(span, name, Vec::new(), ast::ItemStruct(@struct_def, generics))
    }

    fn item_mod(&self, span: Span, inner_span: Span, name: Ident,
                attrs: Vec<ast::Attribute> ,
                vi: Vec<ast::ViewItem> ,
                items: Vec<@ast::Item> ) -> @ast::Item {
        self.item(
            span,
            name,
            attrs,
            ast::ItemMod(ast::Mod {
                inner: inner_span,
                view_items: vi,
                items: items,
            })
        )
    }

    fn item_ty_poly(&self, span: Span, name: Ident, ty: P<ast::Ty>,
                    generics: Generics) -> @ast::Item {
        self.item(span, name, Vec::new(), ast::ItemTy(ty, generics))
    }

    fn item_ty(&self, span: Span, name: Ident, ty: P<ast::Ty>) -> @ast::Item {
        self.item_ty_poly(span, name, ty, ast_util::empty_generics())
    }

    fn attribute(&self, sp: Span, mi: @ast::MetaItem) -> ast::Attribute {
        respan(sp, ast::Attribute_ {
            id: attr::mk_attr_id(),
            style: ast::AttrOuter,
            value: mi,
            is_sugared_doc: false,
        })
    }

    fn meta_word(&self, sp: Span, w: InternedString) -> @ast::MetaItem {
        @respan(sp, ast::MetaWord(w))
    }
    fn meta_list(&self,
                 sp: Span,
                 name: InternedString,
                 mis: Vec<@ast::MetaItem> )
                 -> @ast::MetaItem {
        @respan(sp, ast::MetaList(name, mis))
    }
    fn meta_name_value(&self,
                       sp: Span,
                       name: InternedString,
                       value: ast::Lit_)
                       -> @ast::MetaItem {
        @respan(sp, ast::MetaNameValue(name, respan(sp, value)))
    }

    fn view_use(&self, sp: Span,
                vis: ast::Visibility, vp: @ast::ViewPath) -> ast::ViewItem {
        ast::ViewItem {
            node: ast::ViewItemUse(vp),
            attrs: Vec::new(),
            vis: vis,
            span: sp
        }
    }

    fn view_use_simple(&self, sp: Span, vis: ast::Visibility, path: ast::Path) -> ast::ViewItem {
        let last = path.segments.last().unwrap().identifier;
        self.view_use_simple_(sp, vis, last, path)
    }

    fn view_use_simple_(&self, sp: Span, vis: ast::Visibility,
                        ident: ast::Ident, path: ast::Path) -> ast::ViewItem {
        self.view_use(sp, vis,
                      @respan(sp,
                           ast::ViewPathSimple(ident,
                                               path,
                                               ast::DUMMY_NODE_ID)))
    }

    fn view_use_list(&self, sp: Span, vis: ast::Visibility,
                     path: Vec<ast::Ident> , imports: &[ast::Ident]) -> ast::ViewItem {
        let imports = imports.iter().map(|id| {
            respan(sp, ast::PathListIdent_ { name: *id, id: ast::DUMMY_NODE_ID })
        }).collect();

        self.view_use(sp, vis,
                      @respan(sp,
                           ast::ViewPathList(self.path(sp, path),
                                             imports,
                                             ast::DUMMY_NODE_ID)))
    }

    fn view_use_glob(&self, sp: Span,
                     vis: ast::Visibility, path: Vec<ast::Ident> ) -> ast::ViewItem {
        self.view_use(sp, vis,
                      @respan(sp,
                           ast::ViewPathGlob(self.path(sp, path), ast::DUMMY_NODE_ID)))
    }
}

struct Duplicator<'a> {
    cx: &'a ExtCtxt<'a>,
}

impl<'a> Folder for Duplicator<'a> {
    fn new_id(&mut self, _: NodeId) -> NodeId {
        ast::DUMMY_NODE_ID
    }
}

pub trait Duplicate {
    //
    // Duplication functions
    //
    // These functions just duplicate AST nodes.
    //

    fn duplicate(&self, cx: &ExtCtxt) -> Self;
}

impl Duplicate for @ast::Expr {
    fn duplicate(&self, cx: &ExtCtxt) -> @ast::Expr {
        let mut folder = Duplicator {
            cx: cx,
        };
        folder.fold_expr(*self)
    }
}
