// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! # Categorization
//!
//! The job of the categorization module is to analyze an expression to
//! determine what kind of memory is used in evaluating it (for example,
//! where dereferences occur and what kind of pointer is dereferenced;
//! whether the memory is mutable; etc)
//!
//! Categorization effectively transforms all of our expressions into
//! expressions of the following forms (the actual enum has many more
//! possibilities, naturally, but they are all variants of these base
//! forms):
//!
//!     E = rvalue    // some computed rvalue
//!       | x         // address of a local variable or argument
//!       | *E        // deref of a ptr
//!       | E.comp    // access to an interior component
//!
//! Imagine a routine ToAddr(Expr) that evaluates an expression and returns an
//! address where the result is to be found.  If Expr is an lvalue, then this
//! is the address of the lvalue.  If Expr is an rvalue, this is the address of
//! some temporary spot in memory where the result is stored.
//!
//! Now, cat_expr() classifies the expression Expr and the address A=ToAddr(Expr)
//! as follows:
//!
//! - cat: what kind of expression was this?  This is a subset of the
//!   full expression forms which only includes those that we care about
//!   for the purpose of the analysis.
//! - mutbl: mutability of the address A
//! - ty: the type of data found at the address A
//!
//! The resulting categorization tree differs somewhat from the expressions
//! themselves.  For example, auto-derefs are explicit.  Also, an index a[b] is
//! decomposed into two operations: a dereference to reach the array data and
//! then an index to jump forward to the relevant item.
//!
//! ## By-reference upvars
//!
//! One part of the translation which may be non-obvious is that we translate
//! closure upvars into the dereference of a borrowed pointer; this more closely
//! resembles the runtime translation. So, for example, if we had:
//!
//!     let mut x = 3;
//!     let y = 5;
//!     let inc = || x += y;
//!
//! Then when we categorize `x` (*within* the closure) we would yield a
//! result of `*x'`, effectively, where `x'` is a `cat_upvar` reference
//! tied to `x`. The type of `x'` will be a borrowed pointer.

#![allow(non_camel_case_types)]

pub use self::PointerKind::*;
pub use self::InteriorKind::*;
pub use self::FieldName::*;
pub use self::ElementKind::*;
pub use self::MutabilityCategory::*;
pub use self::InteriorSafety::*;
pub use self::AliasableReason::*;
pub use self::Note::*;
pub use self::deref_kind::*;
pub use self::categorization::*;

use middle::check_const;
use middle::def;
use middle::region;
use middle::ty::{self, Ty};
use util::nodemap::{NodeMap};
use util::ppaux::{Repr, UserString};

use syntax::ast::{MutImmutable, MutMutable};
use syntax::ast;
use syntax::ast_map;
use syntax::codemap::Span;
use syntax::print::pprust;
use syntax::parse::token;

use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, PartialEq, Debug)]
pub enum categorization<'tcx> {
    cat_rvalue(ty::Region),                    // temporary val, argument is its scope
    cat_static_item,
    cat_upvar(Upvar),                          // upvar referenced by closure env
    cat_local(ast::NodeId),                    // local variable
    cat_deref(cmt<'tcx>, uint, PointerKind),   // deref of a ptr
    cat_interior(cmt<'tcx>, InteriorKind),     // something interior: field, tuple, etc
    cat_downcast(cmt<'tcx>, ast::DefId),       // selects a particular enum variant (*1)

    // (*1) downcast is only required if the enum has more than one variant
}

// Represents any kind of upvar
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Upvar {
    pub id: ty::UpvarId,
    pub kind: ty::ClosureKind
}

// different kinds of pointers:
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PointerKind {
    /// `Box<T>`
    Unique,

    /// `&T`
    BorrowedPtr(ty::BorrowKind, ty::Region),

    /// `*T`
    UnsafePtr(ast::Mutability),

    /// Implicit deref of the `&T` that results from an overloaded index `[]`.
    Implicit(ty::BorrowKind, ty::Region),
}

// We use the term "interior" to mean "something reachable from the
// base without a pointer dereference", e.g. a field
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum InteriorKind {
    InteriorField(FieldName),
    InteriorElement(InteriorOffsetKind, ElementKind),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FieldName {
    NamedField(ast::Name),
    PositionalField(uint)
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum InteriorOffsetKind {
    Index,            // e.g. `array_expr[index_expr]`
    Pattern,          // e.g. `fn foo([_, a, _, _]: [A; 4]) { ... }`
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ElementKind {
    VecElement,
    OtherElement,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MutabilityCategory {
    McImmutable, // Immutable.
    McDeclared,  // Directly declared as mutable.
    McInherited, // Inherited from the fact that owner is mutable.
}

// A note about the provenance of a `cmt`.  This is used for
// special-case handling of upvars such as mutability inference.
// Upvar categorization can generate a variable number of nested
// derefs.  The note allows detecting them without deep pattern
// matching on the categorization.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Note {
    NoteClosureEnv(ty::UpvarId), // Deref through closure env
    NoteUpvarRef(ty::UpvarId),   // Deref through by-ref upvar
    NoteNone                     // Nothing special
}

// `cmt`: "Category, Mutability, and Type".
//
// a complete categorization of a value indicating where it originated
// and how it is located, as well as the mutability of the memory in
// which the value is stored.
//
// *WARNING* The field `cmt.type` is NOT necessarily the same as the
// result of `node_id_to_type(cmt.id)`. This is because the `id` is
// always the `id` of the node producing the type; in an expression
// like `*x`, the type of this deref node is the deref'd type (`T`),
// but in a pattern like `@x`, the `@x` pattern is again a
// dereference, but its type is the type *before* the dereference
// (`@T`). So use `cmt.ty` to find the type of the value in a consistent
// fashion. For more details, see the method `cat_pattern`
#[derive(Clone, PartialEq, Debug)]
pub struct cmt_<'tcx> {
    pub id: ast::NodeId,           // id of expr/pat producing this value
    pub span: Span,                // span of same expr/pat
    pub cat: categorization<'tcx>, // categorization of expr
    pub mutbl: MutabilityCategory, // mutability of expr as lvalue
    pub ty: Ty<'tcx>,              // type of the expr (*see WARNING above*)
    pub note: Note,                // Note about the provenance of this cmt
}

pub type cmt<'tcx> = Rc<cmt_<'tcx>>;

// We pun on *T to mean both actual deref of a ptr as well
// as accessing of components:
#[derive(Copy)]
pub enum deref_kind {
    deref_ptr(PointerKind),
    deref_interior(InteriorKind),
}

type DerefKindContext = Option<InteriorOffsetKind>;

// Categorizes a derefable type.  Note that we include vectors and strings as
// derefable (we model an index as the combination of a deref and then a
// pointer adjustment).
fn deref_kind(t: Ty, context: DerefKindContext) -> McResult<deref_kind> {
    match t.sty {
        ty::ty_uniq(_) => {
            Ok(deref_ptr(Unique))
        }

        ty::ty_rptr(r, mt) => {
            let kind = ty::BorrowKind::from_mutbl(mt.mutbl);
            Ok(deref_ptr(BorrowedPtr(kind, *r)))
        }

        ty::ty_ptr(ref mt) => {
            Ok(deref_ptr(UnsafePtr(mt.mutbl)))
        }

        ty::ty_enum(..) |
        ty::ty_struct(..) => { // newtype
            Ok(deref_interior(InteriorField(PositionalField(0))))
        }

        ty::ty_vec(_, _) | ty::ty_str => {
            // no deref of indexed content without supplying InteriorOffsetKind
            if let Some(context) = context {
                Ok(deref_interior(InteriorElement(context, element_kind(t))))
            } else {
                Err(())
            }
        }

        _ => Err(()),
    }
}

pub trait ast_node {
    fn id(&self) -> ast::NodeId;
    fn span(&self) -> Span;
}

impl ast_node for ast::Expr {
    fn id(&self) -> ast::NodeId { self.id }
    fn span(&self) -> Span { self.span }
}

impl ast_node for ast::Pat {
    fn id(&self) -> ast::NodeId { self.id }
    fn span(&self) -> Span { self.span }
}

pub struct MemCategorizationContext<'t,TYPER:'t> {
    typer: &'t TYPER
}

impl<'t,TYPER:'t> Copy for MemCategorizationContext<'t,TYPER> {}

pub type McResult<T> = Result<T, ()>;

/// The `Typer` trait provides the interface for the mem-categorization
/// module to the results of the type check. It can be used to query
/// the type assigned to an expression node, to inquire after adjustments,
/// and so on.
///
/// This interface is needed because mem-categorization is used from
/// two places: `regionck` and `borrowck`. `regionck` executes before
/// type inference is complete, and hence derives types and so on from
/// intermediate tables.  This also implies that type errors can occur,
/// and hence `node_ty()` and friends return a `Result` type -- any
/// error will propagate back up through the mem-categorization
/// routines.
///
/// In the borrow checker, in contrast, type checking is complete and we
/// know that no errors have occurred, so we simply consult the tcx and we
/// can be sure that only `Ok` results will occur.
pub trait Typer<'tcx> : ty::ClosureTyper<'tcx> {
    fn node_ty(&self, id: ast::NodeId) -> McResult<Ty<'tcx>>;
    fn expr_ty_adjusted(&self, expr: &ast::Expr) -> McResult<Ty<'tcx>>;
    fn type_moves_by_default(&self, span: Span, ty: Ty<'tcx>) -> bool;
    fn node_method_ty(&self, method_call: ty::MethodCall) -> Option<Ty<'tcx>>;
    fn node_method_origin(&self, method_call: ty::MethodCall)
                          -> Option<ty::MethodOrigin<'tcx>>;
    fn adjustments<'a>(&'a self) -> &'a RefCell<NodeMap<ty::AutoAdjustment<'tcx>>>;
    fn is_method_call(&self, id: ast::NodeId) -> bool;
    fn temporary_scope(&self, rvalue_id: ast::NodeId) -> Option<region::CodeExtent>;
    fn upvar_capture(&self, upvar_id: ty::UpvarId) -> Option<ty::UpvarCapture>;
}

impl MutabilityCategory {
    pub fn from_mutbl(m: ast::Mutability) -> MutabilityCategory {
        match m {
            MutImmutable => McImmutable,
            MutMutable => McDeclared
        }
    }

    pub fn from_borrow_kind(borrow_kind: ty::BorrowKind) -> MutabilityCategory {
        match borrow_kind {
            ty::ImmBorrow => McImmutable,
            ty::UniqueImmBorrow => McImmutable,
            ty::MutBorrow => McDeclared,
        }
    }

    pub fn from_pointer_kind(base_mutbl: MutabilityCategory,
                             ptr: PointerKind) -> MutabilityCategory {
        match ptr {
            Unique => {
                base_mutbl.inherit()
            }
            BorrowedPtr(borrow_kind, _) | Implicit(borrow_kind, _) => {
                MutabilityCategory::from_borrow_kind(borrow_kind)
            }
            UnsafePtr(m) => {
                MutabilityCategory::from_mutbl(m)
            }
        }
    }

    fn from_local(tcx: &ty::ctxt, id: ast::NodeId) -> MutabilityCategory {
        match tcx.map.get(id) {
            ast_map::NodeLocal(p) | ast_map::NodeArg(p) => match p.node {
                ast::PatIdent(bind_mode, _, _) => {
                    if bind_mode == ast::BindByValue(ast::MutMutable) {
                        McDeclared
                    } else {
                        McImmutable
                    }
                }
                _ => tcx.sess.span_bug(p.span, "expected identifier pattern")
            },
            _ => tcx.sess.span_bug(tcx.map.span(id), "expected identifier pattern")
        }
    }

    pub fn inherit(&self) -> MutabilityCategory {
        match *self {
            McImmutable => McImmutable,
            McDeclared => McInherited,
            McInherited => McInherited,
        }
    }

    pub fn is_mutable(&self) -> bool {
        match *self {
            McImmutable => false,
            McInherited => true,
            McDeclared => true,
        }
    }

    pub fn is_immutable(&self) -> bool {
        match *self {
            McImmutable => true,
            McDeclared | McInherited => false
        }
    }

    pub fn to_user_str(&self) -> &'static str {
        match *self {
            McDeclared | McInherited => "mutable",
            McImmutable => "immutable",
        }
    }
}

impl<'t,'tcx,TYPER:Typer<'tcx>> MemCategorizationContext<'t,TYPER> {
    pub fn new(typer: &'t TYPER) -> MemCategorizationContext<'t,TYPER> {
        MemCategorizationContext { typer: typer }
    }

    fn tcx(&self) -> &'t ty::ctxt<'tcx> {
        self.typer.tcx()
    }

    fn expr_ty(&self, expr: &ast::Expr) -> McResult<Ty<'tcx>> {
        self.typer.node_ty(expr.id)
    }

    fn expr_ty_adjusted(&self, expr: &ast::Expr) -> McResult<Ty<'tcx>> {
        let unadjusted_ty = try!(self.expr_ty(expr));
        Ok(ty::adjust_ty(self.tcx(), expr.span, expr.id, unadjusted_ty,
                         self.typer.adjustments().borrow().get(&expr.id),
                         |method_call| self.typer.node_method_ty(method_call)))
    }

    fn node_ty(&self, id: ast::NodeId) -> McResult<Ty<'tcx>> {
        self.typer.node_ty(id)
    }

    fn pat_ty(&self, pat: &ast::Pat) -> McResult<Ty<'tcx>> {
        let tcx = self.typer.tcx();
        let base_ty = try!(self.typer.node_ty(pat.id));
        // FIXME (Issue #18207): This code detects whether we are
        // looking at a `ref x`, and if so, figures out what the type
        // *being borrowed* is.  But ideally we would put in a more
        // fundamental fix to this conflated use of the node id.
        let ret_ty = match pat.node {
            ast::PatIdent(ast::BindByRef(_), _, _) => {
                // a bind-by-ref means that the base_ty will be the type of the ident itself,
                // but what we want here is the type of the underlying value being borrowed.
                // So peel off one-level, turning the &T into T.
                match ty::deref(base_ty, false) {
                    Some(t) => t.ty,
                    None => { return Err(()); }
                }
            }
            _ => base_ty,
        };
        debug!("pat_ty(pat={}) base_ty={} ret_ty={}",
               pat.repr(tcx), base_ty.repr(tcx), ret_ty.repr(tcx));
        Ok(ret_ty)
    }

    pub fn cat_expr(&self, expr: &ast::Expr) -> McResult<cmt<'tcx>> {
        match self.typer.adjustments().borrow().get(&expr.id) {
            None => {
                // No adjustments.
                self.cat_expr_unadjusted(expr)
            }

            Some(adjustment) => {
                match *adjustment {
                    ty::AdjustReifyFnPointer(..) |
                    ty::AdjustUnsafeFnPointer(..) => {
                        debug!("cat_expr(AdjustReifyFnPointer): {}",
                               expr.repr(self.tcx()));
                        // Convert a bare fn to a closure by adding NULL env.
                        // Result is an rvalue.
                        let expr_ty = try!(self.expr_ty_adjusted(expr));
                        Ok(self.cat_rvalue_node(expr.id(), expr.span(), expr_ty))
                    }

                    ty::AdjustDerefRef(
                        ty::AutoDerefRef {
                            autoref: Some(_), ..}) => {
                        debug!("cat_expr(AdjustDerefRef): {}",
                               expr.repr(self.tcx()));
                        // Equivalent to &*expr or something similar.
                        // Result is an rvalue.
                        let expr_ty = try!(self.expr_ty_adjusted(expr));
                        Ok(self.cat_rvalue_node(expr.id(), expr.span(), expr_ty))
                    }

                    ty::AdjustDerefRef(
                        ty::AutoDerefRef {
                            autoref: None, autoderefs}) => {
                        // Equivalent to *expr or something similar.
                        self.cat_expr_autoderefd(expr, autoderefs)
                    }
                }
            }
        }
    }

    pub fn cat_expr_autoderefd(&self,
                               expr: &ast::Expr,
                               autoderefs: uint)
                               -> McResult<cmt<'tcx>> {
        let mut cmt = try!(self.cat_expr_unadjusted(expr));
        debug!("cat_expr_autoderefd: autoderefs={}, cmt={}",
               autoderefs,
               cmt.repr(self.tcx()));
        for deref in 1..autoderefs + 1 {
            cmt = try!(self.cat_deref(expr, cmt, deref, None));
        }
        return Ok(cmt);
    }

    pub fn cat_expr_unadjusted(&self, expr: &ast::Expr) -> McResult<cmt<'tcx>> {
        debug!("cat_expr: id={} expr={}", expr.id, expr.repr(self.tcx()));

        let expr_ty = try!(self.expr_ty(expr));
        match expr.node {
          ast::ExprUnary(ast::UnDeref, ref e_base) => {
            let base_cmt = try!(self.cat_expr(&**e_base));
            self.cat_deref(expr, base_cmt, 0, None)
          }

          ast::ExprField(ref base, f_name) => {
            let base_cmt = try!(self.cat_expr(&**base));
            debug!("cat_expr(cat_field): id={} expr={} base={}",
                   expr.id,
                   expr.repr(self.tcx()),
                   base_cmt.repr(self.tcx()));
            Ok(self.cat_field(expr, base_cmt, f_name.node.name, expr_ty))
          }

          ast::ExprTupField(ref base, idx) => {
            let base_cmt = try!(self.cat_expr(&**base));
            Ok(self.cat_tup_field(expr, base_cmt, idx.node, expr_ty))
          }

          ast::ExprIndex(ref base, _) => {
            let method_call = ty::MethodCall::expr(expr.id());
            let context = InteriorOffsetKind::Index;
            match self.typer.node_method_ty(method_call) {
                Some(method_ty) => {
                    // If this is an index implemented by a method call, then it
                    // will include an implicit deref of the result.
                    let ret_ty = self.overloaded_method_return_ty(method_ty);

                    // The index method always returns an `&T`, so
                    // dereference it to find the result type.
                    let elem_ty = match ret_ty.sty {
                        ty::ty_rptr(_, mt) => mt.ty,
                        _ => {
                            debug!("cat_expr_unadjusted: return type of overloaded index is {}?",
                                   ret_ty.repr(self.tcx()));
                            return Err(());
                        }
                    };

                    // The call to index() returns a `&T` value, which
                    // is an rvalue. That is what we will be
                    // dereferencing.
                    let base_cmt = self.cat_rvalue_node(expr.id(), expr.span(), ret_ty);
                    self.cat_deref_common(expr, base_cmt, 1, elem_ty, Some(context), true)
                }
                None => {
                    self.cat_index(expr, try!(self.cat_expr(&**base)), context)
                }
            }
          }

          ast::ExprPath(..) => {
            let def = self.tcx().def_map.borrow()[expr.id].full_def();
            self.cat_def(expr.id, expr.span, expr_ty, def)
          }

          ast::ExprParen(ref e) => {
            self.cat_expr(&**e)
          }

          ast::ExprAddrOf(..) | ast::ExprCall(..) |
          ast::ExprAssign(..) | ast::ExprAssignOp(..) |
          ast::ExprClosure(..) | ast::ExprRet(..) |
          ast::ExprUnary(..) | ast::ExprRange(..) |
          ast::ExprMethodCall(..) | ast::ExprCast(..) |
          ast::ExprVec(..) | ast::ExprTup(..) | ast::ExprIf(..) |
          ast::ExprBinary(..) | ast::ExprWhile(..) |
          ast::ExprBlock(..) | ast::ExprLoop(..) | ast::ExprMatch(..) |
          ast::ExprLit(..) | ast::ExprBreak(..) | ast::ExprMac(..) |
          ast::ExprAgain(..) | ast::ExprStruct(..) | ast::ExprRepeat(..) |
          ast::ExprInlineAsm(..) | ast::ExprBox(..) => {
            Ok(self.cat_rvalue_node(expr.id(), expr.span(), expr_ty))
          }

          ast::ExprIfLet(..) => {
            self.tcx().sess.span_bug(expr.span, "non-desugared ExprIfLet");
          }
          ast::ExprWhileLet(..) => {
            self.tcx().sess.span_bug(expr.span, "non-desugared ExprWhileLet");
          }
          ast::ExprForLoop(..) => {
            self.tcx().sess.span_bug(expr.span, "non-desugared ExprForLoop");
          }
        }
    }

    pub fn cat_def(&self,
                   id: ast::NodeId,
                   span: Span,
                   expr_ty: Ty<'tcx>,
                   def: def::Def)
                   -> McResult<cmt<'tcx>> {
        debug!("cat_def: id={} expr={} def={:?}",
               id, expr_ty.repr(self.tcx()), def);

        match def {
          def::DefStruct(..) | def::DefVariant(..) | def::DefConst(..) |
          def::DefFn(..) | def::DefMethod(..) => {
                Ok(self.cat_rvalue_node(id, span, expr_ty))
          }
          def::DefMod(_) | def::DefForeignMod(_) | def::DefUse(_) |
          def::DefTrait(_) | def::DefTy(..) | def::DefPrimTy(_) |
          def::DefTyParam(..) | def::DefRegion(_) |
          def::DefLabel(_) | def::DefSelfTy(..) |
          def::DefAssociatedTy(..) => {
              Ok(Rc::new(cmt_ {
                  id:id,
                  span:span,
                  cat:cat_static_item,
                  mutbl: McImmutable,
                  ty:expr_ty,
                  note: NoteNone
              }))
          }

          def::DefStatic(_, mutbl) => {
              Ok(Rc::new(cmt_ {
                  id:id,
                  span:span,
                  cat:cat_static_item,
                  mutbl: if mutbl { McDeclared } else { McImmutable},
                  ty:expr_ty,
                  note: NoteNone
              }))
          }

          def::DefUpvar(var_id, fn_node_id) => {
              let ty = try!(self.node_ty(fn_node_id));
              match ty.sty {
                  ty::ty_closure(closure_id, _) => {
                      match self.typer.closure_kind(closure_id) {
                          Some(kind) => {
                              self.cat_upvar(id, span, var_id, fn_node_id, kind)
                          }
                          None => {
                              self.tcx().sess.span_bug(
                                  span,
                                  &*format!("No closure kind for {:?}", closure_id));
                          }
                      }
                  }
                  _ => {
                      self.tcx().sess.span_bug(
                          span,
                          &format!("Upvar of non-closure {} - {}",
                                  fn_node_id,
                                  ty.repr(self.tcx())));
                  }
              }
          }

          def::DefLocal(vid) => {
            Ok(Rc::new(cmt_ {
                id: id,
                span: span,
                cat: cat_local(vid),
                mutbl: MutabilityCategory::from_local(self.tcx(), vid),
                ty: expr_ty,
                note: NoteNone
            }))
          }
        }
    }

    // Categorize an upvar, complete with invisible derefs of closure
    // environment and upvar reference as appropriate.
    fn cat_upvar(&self,
                 id: ast::NodeId,
                 span: Span,
                 var_id: ast::NodeId,
                 fn_node_id: ast::NodeId,
                 kind: ty::ClosureKind)
                 -> McResult<cmt<'tcx>>
    {
        // An upvar can have up to 3 components. We translate first to a
        // `cat_upvar`, which is itself a fiction -- it represents the reference to the
        // field from the environment.
        //
        // `cat_upvar`.  Next, we add a deref through the implicit
        // environment pointer with an anonymous free region 'env and
        // appropriate borrow kind for closure kinds that take self by
        // reference.  Finally, if the upvar was captured
        // by-reference, we add a deref through that reference.  The
        // region of this reference is an inference variable 'up that
        // was previously generated and recorded in the upvar borrow
        // map.  The borrow kind bk is inferred by based on how the
        // upvar is used.
        //
        // This results in the following table for concrete closure
        // types:
        //
        //                | move                 | ref
        // ---------------+----------------------+-------------------------------
        // Fn             | copied -> &'env      | upvar -> &'env -> &'up bk
        // FnMut          | copied -> &'env mut  | upvar -> &'env mut -> &'up bk
        // FnOnce         | copied               | upvar -> &'up bk

        let upvar_id = ty::UpvarId { var_id: var_id,
                                     closure_expr_id: fn_node_id };
        let var_ty = try!(self.node_ty(var_id));

        // Mutability of original variable itself
        let var_mutbl = MutabilityCategory::from_local(self.tcx(), var_id);

        // Construct the upvar. This represents access to the field
        // from the environment (perhaps we should eventually desugar
        // this field further, but it will do for now).
        let cmt_result = cmt_ {
            id: id,
            span: span,
            cat: cat_upvar(Upvar {id: upvar_id, kind: kind}),
            mutbl: var_mutbl,
            ty: var_ty,
            note: NoteNone
        };

        // If this is a `FnMut` or `Fn` closure, then the above is
        // conceptually a `&mut` or `&` reference, so we have to add a
        // deref.
        let cmt_result = match kind {
            ty::FnOnceClosureKind => {
                cmt_result
            }
            ty::FnMutClosureKind => {
                self.env_deref(id, span, upvar_id, var_mutbl, ty::MutBorrow, cmt_result)
            }
            ty::FnClosureKind => {
                self.env_deref(id, span, upvar_id, var_mutbl, ty::ImmBorrow, cmt_result)
            }
        };

        // If this is a by-ref capture, then the upvar we loaded is
        // actually a reference, so we have to add an implicit deref
        // for that.
        let upvar_id = ty::UpvarId { var_id: var_id,
                                     closure_expr_id: fn_node_id };
        let upvar_capture = self.typer.upvar_capture(upvar_id).unwrap();
        let cmt_result = match upvar_capture {
            ty::UpvarCapture::ByValue => {
                cmt_result
            }
            ty::UpvarCapture::ByRef(upvar_borrow) => {
                let ptr = BorrowedPtr(upvar_borrow.kind, upvar_borrow.region);
                cmt_ {
                    id: id,
                    span: span,
                    cat: cat_deref(Rc::new(cmt_result), 0, ptr),
                    mutbl: MutabilityCategory::from_borrow_kind(upvar_borrow.kind),
                    ty: var_ty,
                    note: NoteUpvarRef(upvar_id)
                }
            }
        };

        Ok(Rc::new(cmt_result))
    }

    fn env_deref(&self,
                 id: ast::NodeId,
                 span: Span,
                 upvar_id: ty::UpvarId,
                 upvar_mutbl: MutabilityCategory,
                 env_borrow_kind: ty::BorrowKind,
                 cmt_result: cmt_<'tcx>)
                 -> cmt_<'tcx>
    {
        // Look up the node ID of the closure body so we can construct
        // a free region within it
        let fn_body_id = {
            let fn_expr = match self.tcx().map.find(upvar_id.closure_expr_id) {
                Some(ast_map::NodeExpr(e)) => e,
                _ => unreachable!()
            };

            match fn_expr.node {
                ast::ExprClosure(_, _, ref body) => body.id,
                _ => unreachable!()
            }
        };

        // Region of environment pointer
        let env_region = ty::ReFree(ty::FreeRegion {
            // The environment of a closure is guaranteed to
            // outlive any bindings introduced in the body of the
            // closure itself.
            scope: region::DestructionScopeData::new(fn_body_id),
            bound_region: ty::BrEnv
        });

        let env_ptr = BorrowedPtr(env_borrow_kind, env_region);

        let var_ty = cmt_result.ty;

        // We need to add the env deref.  This means
        // that the above is actually immutable and
        // has a ref type.  However, nothing should
        // actually look at the type, so we can get
        // away with stuffing a `ty_err` in there
        // instead of bothering to construct a proper
        // one.
        let cmt_result = cmt_ {
            mutbl: McImmutable,
            ty: self.tcx().types.err,
            ..cmt_result
        };

        let mut deref_mutbl = MutabilityCategory::from_borrow_kind(env_borrow_kind);

        // Issue #18335. If variable is declared as immutable, override the
        // mutability from the environment and substitute an `&T` anyway.
        match upvar_mutbl {
            McImmutable => { deref_mutbl = McImmutable; }
            McDeclared | McInherited => { }
        }

        cmt_ {
            id: id,
            span: span,
            cat: cat_deref(Rc::new(cmt_result), 0, env_ptr),
            mutbl: deref_mutbl,
            ty: var_ty,
            note: NoteClosureEnv(upvar_id)
        }
    }

    pub fn cat_rvalue_node(&self,
                           id: ast::NodeId,
                           span: Span,
                           expr_ty: Ty<'tcx>)
                           -> cmt<'tcx> {
        let qualif = self.tcx().const_qualif_map.borrow().get(&id).cloned()
                               .unwrap_or(check_const::NOT_CONST);

        // Only promote `[T; 0]` before an RFC for rvalue promotions
        // is accepted.
        let qualif = match expr_ty.sty {
            ty::ty_vec(_, Some(0)) => qualif,
            _ => check_const::NOT_CONST
        };

        let re = match qualif & check_const::NON_STATIC_BORROWS {
            check_const::PURE_CONST => {
                // Constant rvalues get promoted to 'static.
                ty::ReStatic
            }
            _ => {
                match self.typer.temporary_scope(id) {
                    Some(scope) => ty::ReScope(scope),
                    None => ty::ReStatic
                }
            }
        };
        self.cat_rvalue(id, span, re, expr_ty)
    }

    pub fn cat_rvalue(&self,
                      cmt_id: ast::NodeId,
                      span: Span,
                      temp_scope: ty::Region,
                      expr_ty: Ty<'tcx>) -> cmt<'tcx> {
        Rc::new(cmt_ {
            id:cmt_id,
            span:span,
            cat:cat_rvalue(temp_scope),
            mutbl:McDeclared,
            ty:expr_ty,
            note: NoteNone
        })
    }

    pub fn cat_field<N:ast_node>(&self,
                                 node: &N,
                                 base_cmt: cmt<'tcx>,
                                 f_name: ast::Name,
                                 f_ty: Ty<'tcx>)
                                 -> cmt<'tcx> {
        Rc::new(cmt_ {
            id: node.id(),
            span: node.span(),
            mutbl: base_cmt.mutbl.inherit(),
            cat: cat_interior(base_cmt, InteriorField(NamedField(f_name))),
            ty: f_ty,
            note: NoteNone
        })
    }

    pub fn cat_tup_field<N:ast_node>(&self,
                                     node: &N,
                                     base_cmt: cmt<'tcx>,
                                     f_idx: uint,
                                     f_ty: Ty<'tcx>)
                                     -> cmt<'tcx> {
        Rc::new(cmt_ {
            id: node.id(),
            span: node.span(),
            mutbl: base_cmt.mutbl.inherit(),
            cat: cat_interior(base_cmt, InteriorField(PositionalField(f_idx))),
            ty: f_ty,
            note: NoteNone
        })
    }

    fn cat_deref<N:ast_node>(&self,
                             node: &N,
                             base_cmt: cmt<'tcx>,
                             deref_cnt: uint,
                             deref_context: DerefKindContext)
                             -> McResult<cmt<'tcx>> {
        let adjustment = match self.typer.adjustments().borrow().get(&node.id()) {
            Some(adj) if ty::adjust_is_object(adj) => ty::AutoObject,
            _ if deref_cnt != 0 => ty::AutoDeref(deref_cnt),
            _ => ty::NoAdjustment
        };

        let method_call = ty::MethodCall {
            expr_id: node.id(),
            adjustment: adjustment
        };
        let method_ty = self.typer.node_method_ty(method_call);

        debug!("cat_deref: method_call={:?} method_ty={:?}",
               method_call, method_ty.map(|ty| ty.repr(self.tcx())));

        let base_cmt = match method_ty {
            Some(method_ty) => {
                let ref_ty =
                    ty::no_late_bound_regions(
                        self.tcx(), &ty::ty_fn_ret(method_ty)).unwrap().unwrap();
                self.cat_rvalue_node(node.id(), node.span(), ref_ty)
            }
            None => base_cmt
        };
        let base_cmt_ty = base_cmt.ty;
        match ty::deref(base_cmt_ty, true) {
            Some(mt) => self.cat_deref_common(node, base_cmt, deref_cnt,
                                              mt.ty,
                                              deref_context,
                                              /* implicit: */ false),
            None => {
                debug!("Explicit deref of non-derefable type: {}",
                       base_cmt_ty.repr(self.tcx()));
                return Err(());
            }
        }
    }

    fn cat_deref_common<N:ast_node>(&self,
                                    node: &N,
                                    base_cmt: cmt<'tcx>,
                                    deref_cnt: uint,
                                    deref_ty: Ty<'tcx>,
                                    deref_context: DerefKindContext,
                                    implicit: bool)
                                    -> McResult<cmt<'tcx>>
    {
        let (m, cat) = match try!(deref_kind(base_cmt.ty, deref_context)) {
            deref_ptr(ptr) => {
                let ptr = if implicit {
                    match ptr {
                        BorrowedPtr(bk, r) => Implicit(bk, r),
                        _ => self.tcx().sess.span_bug(node.span(),
                            "Implicit deref of non-borrowed pointer")
                    }
                } else {
                    ptr
                };
                // for unique ptrs, we inherit mutability from the
                // owning reference.
                (MutabilityCategory::from_pointer_kind(base_cmt.mutbl, ptr),
                 cat_deref(base_cmt, deref_cnt, ptr))
            }
            deref_interior(interior) => {
                (base_cmt.mutbl.inherit(), cat_interior(base_cmt, interior))
            }
        };
        Ok(Rc::new(cmt_ {
            id: node.id(),
            span: node.span(),
            cat: cat,
            mutbl: m,
            ty: deref_ty,
            note: NoteNone
        }))
    }

    pub fn cat_index<N:ast_node>(&self,
                                 elt: &N,
                                 mut base_cmt: cmt<'tcx>,
                                 context: InteriorOffsetKind)
                                 -> McResult<cmt<'tcx>> {
        //! Creates a cmt for an indexing operation (`[]`).
        //!
        //! One subtle aspect of indexing that may not be
        //! immediately obvious: for anything other than a fixed-length
        //! vector, an operation like `x[y]` actually consists of two
        //! disjoint (from the point of view of borrowck) operations.
        //! The first is a deref of `x` to create a pointer `p` that points
        //! at the first element in the array. The second operation is
        //! an index which adds `y*sizeof(T)` to `p` to obtain the
        //! pointer to `x[y]`. `cat_index` will produce a resulting
        //! cmt containing both this deref and the indexing,
        //! presuming that `base_cmt` is not of fixed-length type.
        //!
        //! # Parameters
        //! - `elt`: the AST node being indexed
        //! - `base_cmt`: the cmt of `elt`

        let method_call = ty::MethodCall::expr(elt.id());
        let method_ty = self.typer.node_method_ty(method_call);

        let element_ty = match method_ty {
            Some(method_ty) => {
                let ref_ty = self.overloaded_method_return_ty(method_ty);
                base_cmt = self.cat_rvalue_node(elt.id(), elt.span(), ref_ty);

                // FIXME(#20649) -- why are we using the `self_ty` as the element type...?
                let self_ty = ty::ty_fn_sig(method_ty).input(0);
                ty::no_late_bound_regions(self.tcx(), &self_ty).unwrap()
            }
            None => {
                match ty::array_element_ty(self.tcx(), base_cmt.ty) {
                    Some(ty) => ty,
                    None => {
                        return Err(());
                    }
                }
            }
        };

        let m = base_cmt.mutbl.inherit();
        return Ok(interior(elt, base_cmt.clone(), base_cmt.ty,
                           m, context, element_ty));

        fn interior<'tcx, N: ast_node>(elt: &N,
                                       of_cmt: cmt<'tcx>,
                                       vec_ty: Ty<'tcx>,
                                       mutbl: MutabilityCategory,
                                       context: InteriorOffsetKind,
                                       element_ty: Ty<'tcx>) -> cmt<'tcx>
        {
            let interior_elem = InteriorElement(context, element_kind(vec_ty));
            Rc::new(cmt_ {
                id:elt.id(),
                span:elt.span(),
                cat:cat_interior(of_cmt, interior_elem),
                mutbl:mutbl,
                ty:element_ty,
                note: NoteNone
            })
        }
    }

    // Takes either a vec or a reference to a vec and returns the cmt for the
    // underlying vec.
    fn deref_vec<N:ast_node>(&self,
                             elt: &N,
                             base_cmt: cmt<'tcx>,
                             context: InteriorOffsetKind)
                             -> McResult<cmt<'tcx>>
    {
        match try!(deref_kind(base_cmt.ty, Some(context))) {
            deref_ptr(ptr) => {
                // for unique ptrs, we inherit mutability from the
                // owning reference.
                let m = MutabilityCategory::from_pointer_kind(base_cmt.mutbl, ptr);

                // the deref is explicit in the resulting cmt
                Ok(Rc::new(cmt_ {
                    id:elt.id(),
                    span:elt.span(),
                    cat:cat_deref(base_cmt.clone(), 0, ptr),
                    mutbl:m,
                    ty: match ty::deref(base_cmt.ty, false) {
                        Some(mt) => mt.ty,
                        None => self.tcx().sess.bug("Found non-derefable type")
                    },
                    note: NoteNone
                }))
            }

            deref_interior(_) => {
                Ok(base_cmt)
            }
        }
    }

    /// Given a pattern P like: `[_, ..Q, _]`, where `vec_cmt` is the cmt for `P`, `slice_pat` is
    /// the pattern `Q`, returns:
    ///
    /// * a cmt for `Q`
    /// * the mutability and region of the slice `Q`
    ///
    /// These last two bits of info happen to be things that borrowck needs.
    pub fn cat_slice_pattern(&self,
                             vec_cmt: cmt<'tcx>,
                             slice_pat: &ast::Pat)
                             -> McResult<(cmt<'tcx>, ast::Mutability, ty::Region)> {
        let slice_ty = try!(self.node_ty(slice_pat.id));
        let (slice_mutbl, slice_r) = vec_slice_info(self.tcx(),
                                                    slice_pat,
                                                    slice_ty);
        let context = InteriorOffsetKind::Pattern;
        let cmt_vec = try!(self.deref_vec(slice_pat, vec_cmt, context));
        let cmt_slice = try!(self.cat_index(slice_pat, cmt_vec, context));
        return Ok((cmt_slice, slice_mutbl, slice_r));

        /// In a pattern like [a, b, ..c], normally `c` has slice type, but if you have [a, b,
        /// ..ref c], then the type of `ref c` will be `&&[]`, so to extract the slice details we
        /// have to recurse through rptrs.
        fn vec_slice_info(tcx: &ty::ctxt,
                          pat: &ast::Pat,
                          slice_ty: Ty)
                          -> (ast::Mutability, ty::Region) {
            match slice_ty.sty {
                ty::ty_rptr(r, ref mt) => match mt.ty.sty {
                    ty::ty_vec(_, None) => (mt.mutbl, *r),
                    _ => vec_slice_info(tcx, pat, mt.ty),
                },

                _ => {
                    tcx.sess.span_bug(pat.span,
                                      "type of slice pattern is not a slice");
                }
            }
        }
    }

    pub fn cat_imm_interior<N:ast_node>(&self,
                                        node: &N,
                                        base_cmt: cmt<'tcx>,
                                        interior_ty: Ty<'tcx>,
                                        interior: InteriorKind)
                                        -> cmt<'tcx> {
        Rc::new(cmt_ {
            id: node.id(),
            span: node.span(),
            mutbl: base_cmt.mutbl.inherit(),
            cat: cat_interior(base_cmt, interior),
            ty: interior_ty,
            note: NoteNone
        })
    }

    pub fn cat_downcast<N:ast_node>(&self,
                                    node: &N,
                                    base_cmt: cmt<'tcx>,
                                    downcast_ty: Ty<'tcx>,
                                    variant_did: ast::DefId)
                                    -> cmt<'tcx> {
        Rc::new(cmt_ {
            id: node.id(),
            span: node.span(),
            mutbl: base_cmt.mutbl.inherit(),
            cat: cat_downcast(base_cmt, variant_did),
            ty: downcast_ty,
            note: NoteNone
        })
    }

    pub fn cat_pattern<F>(&self, cmt: cmt<'tcx>, pat: &ast::Pat, mut op: F) -> McResult<()>
        where F: FnMut(&MemCategorizationContext<'t, TYPER>, cmt<'tcx>, &ast::Pat),
    {
        self.cat_pattern_(cmt, pat, &mut op)
    }

    // FIXME(#19596) This is a workaround, but there should be a better way to do this
    fn cat_pattern_<F>(&self, cmt: cmt<'tcx>, pat: &ast::Pat, op: &mut F)
                       -> McResult<()>
        where F : FnMut(&MemCategorizationContext<'t, TYPER>, cmt<'tcx>, &ast::Pat),
    {
        // Here, `cmt` is the categorization for the value being
        // matched and pat is the pattern it is being matched against.
        //
        // In general, the way that this works is that we walk down
        // the pattern, constructing a cmt that represents the path
        // that will be taken to reach the value being matched.
        //
        // When we encounter named bindings, we take the cmt that has
        // been built up and pass it off to guarantee_valid() so that
        // we can be sure that the binding will remain valid for the
        // duration of the arm.
        //
        // (*2) There is subtlety concerning the correspondence between
        // pattern ids and types as compared to *expression* ids and
        // types. This is explained briefly. on the definition of the
        // type `cmt`, so go off and read what it says there, then
        // come back and I'll dive into a bit more detail here. :) OK,
        // back?
        //
        // In general, the id of the cmt should be the node that
        // "produces" the value---patterns aren't executable code
        // exactly, but I consider them to "execute" when they match a
        // value, and I consider them to produce the value that was
        // matched. So if you have something like:
        //
        //     let x = @@3;
        //     match x {
        //       @@y { ... }
        //     }
        //
        // In this case, the cmt and the relevant ids would be:
        //
        //     CMT             Id                  Type of Id Type of cmt
        //
        //     local(x)->@->@
        //     ^~~~~~~^        `x` from discr      @@int      @@int
        //     ^~~~~~~~~~^     `@@y` pattern node  @@int      @int
        //     ^~~~~~~~~~~~~^  `@y` pattern node   @int       int
        //
        // You can see that the types of the id and the cmt are in
        // sync in the first line, because that id is actually the id
        // of an expression. But once we get to pattern ids, the types
        // step out of sync again. So you'll see below that we always
        // get the type of the *subpattern* and use that.

        debug!("cat_pattern: id={} pat={} cmt={}",
               pat.id, pprust::pat_to_string(pat),
               cmt.repr(self.tcx()));

        (*op)(self, cmt.clone(), pat);

        let opt_def = self.tcx().def_map.borrow().get(&pat.id).map(|d| d.full_def());

        // Note: This goes up here (rather than within the PatEnum arm
        // alone) because struct patterns can refer to struct types or
        // to struct variants within enums.
        let cmt = match opt_def {
            Some(def::DefVariant(enum_did, variant_did, _))
                // univariant enums do not need downcasts
                if !ty::enum_is_univariant(self.tcx(), enum_did) => {
                    self.cat_downcast(pat, cmt.clone(), cmt.ty, variant_did)
                }
            _ => cmt
        };

        match pat.node {
          ast::PatWild(_) => {
            // _
          }

          ast::PatEnum(_, None) => {
            // variant(..)
          }
          ast::PatEnum(_, Some(ref subpats)) => {
            match opt_def {
                Some(def::DefVariant(..)) => {
                    // variant(x, y, z)
                    for (i, subpat) in subpats.iter().enumerate() {
                        let subpat_ty = try!(self.pat_ty(&**subpat)); // see (*2)

                        let subcmt =
                            self.cat_imm_interior(
                                pat, cmt.clone(), subpat_ty,
                                InteriorField(PositionalField(i)));

                        try!(self.cat_pattern_(subcmt, &**subpat, op));
                    }
                }
                Some(def::DefStruct(..)) => {
                    for (i, subpat) in subpats.iter().enumerate() {
                        let subpat_ty = try!(self.pat_ty(&**subpat)); // see (*2)
                        let cmt_field =
                            self.cat_imm_interior(
                                pat, cmt.clone(), subpat_ty,
                                InteriorField(PositionalField(i)));
                        try!(self.cat_pattern_(cmt_field, &**subpat, op));
                    }
                }
                Some(def::DefConst(..)) => {
                    for subpat in subpats {
                        try!(self.cat_pattern_(cmt.clone(), &**subpat, op));
                    }
                }
                _ => {
                    self.tcx().sess.span_bug(
                        pat.span,
                        "enum pattern didn't resolve to enum or struct");
                }
            }
          }

          ast::PatIdent(_, _, Some(ref subpat)) => {
              try!(self.cat_pattern_(cmt, &**subpat, op));
          }

          ast::PatIdent(_, _, None) => {
              // nullary variant or identifier: ignore
          }

          ast::PatStruct(_, ref field_pats, _) => {
            // {f1: p1, ..., fN: pN}
            for fp in field_pats {
                let field_ty = try!(self.pat_ty(&*fp.node.pat)); // see (*2)
                let cmt_field = self.cat_field(pat, cmt.clone(), fp.node.ident.name, field_ty);
                try!(self.cat_pattern_(cmt_field, &*fp.node.pat, op));
            }
          }

          ast::PatTup(ref subpats) => {
            // (p1, ..., pN)
            for (i, subpat) in subpats.iter().enumerate() {
                let subpat_ty = try!(self.pat_ty(&**subpat)); // see (*2)
                let subcmt =
                    self.cat_imm_interior(
                        pat, cmt.clone(), subpat_ty,
                        InteriorField(PositionalField(i)));
                try!(self.cat_pattern_(subcmt, &**subpat, op));
            }
          }

          ast::PatBox(ref subpat) | ast::PatRegion(ref subpat, _) => {
            // box p1, &p1, &mut p1.  we can ignore the mutability of
            // PatRegion since that information is already contained
            // in the type.
            let subcmt = try!(self.cat_deref(pat, cmt, 0, None));
              try!(self.cat_pattern_(subcmt, &**subpat, op));
          }

          ast::PatVec(ref before, ref slice, ref after) => {
              let context = InteriorOffsetKind::Pattern;
              let vec_cmt = try!(self.deref_vec(pat, cmt, context));
              let elt_cmt = try!(self.cat_index(pat, vec_cmt, context));
              for before_pat in before {
                  try!(self.cat_pattern_(elt_cmt.clone(), &**before_pat, op));
              }
              if let Some(ref slice_pat) = *slice {
                  let slice_ty = try!(self.pat_ty(&**slice_pat));
                  let slice_cmt = self.cat_rvalue_node(pat.id(), pat.span(), slice_ty);
                  try!(self.cat_pattern_(slice_cmt, &**slice_pat, op));
              }
              for after_pat in after {
                  try!(self.cat_pattern_(elt_cmt.clone(), &**after_pat, op));
              }
          }

          ast::PatLit(_) | ast::PatRange(_, _) => {
              /*always ok*/
          }

          ast::PatMac(_) => {
              self.tcx().sess.span_bug(pat.span, "unexpanded macro");
          }
        }

        Ok(())
    }

    fn overloaded_method_return_ty(&self,
                                   method_ty: Ty<'tcx>)
                                   -> Ty<'tcx>
    {
        // When we process an overloaded `*` or `[]` etc, we often
        // need to extract the return type of the method. These method
        // types are generated by method resolution and always have
        // all late-bound regions fully instantiated, so we just want
        // to skip past the binder.
        ty::no_late_bound_regions(self.tcx(), &ty::ty_fn_ret(method_ty))
           .unwrap()
           .unwrap() // overloaded ops do not diverge, either
    }
}

#[derive(Copy)]
pub enum InteriorSafety {
    InteriorUnsafe,
    InteriorSafe
}

#[derive(Copy)]
pub enum AliasableReason {
    AliasableBorrowed,
    AliasableClosure(ast::NodeId), // Aliasable due to capture Fn closure env
    AliasableOther,
    AliasableStatic(InteriorSafety),
    AliasableStaticMut(InteriorSafety),
}

impl<'tcx> cmt_<'tcx> {
    pub fn guarantor(&self) -> cmt<'tcx> {
        //! Returns `self` after stripping away any owned pointer derefs or
        //! interior content. The return value is basically the `cmt` which
        //! determines how long the value in `self` remains live.

        match self.cat {
            cat_rvalue(..) |
            cat_static_item |
            cat_local(..) |
            cat_deref(_, _, UnsafePtr(..)) |
            cat_deref(_, _, BorrowedPtr(..)) |
            cat_deref(_, _, Implicit(..)) |
            cat_upvar(..) => {
                Rc::new((*self).clone())
            }
            cat_downcast(ref b, _) |
            cat_interior(ref b, _) |
            cat_deref(ref b, _, Unique) => {
                b.guarantor()
            }
        }
    }

    /// Returns `Some(_)` if this lvalue represents a freely aliasable pointer type.
    pub fn freely_aliasable(&self, ctxt: &ty::ctxt<'tcx>)
                            -> Option<AliasableReason> {
        // Maybe non-obvious: copied upvars can only be considered
        // non-aliasable in once closures, since any other kind can be
        // aliased and eventually recused.

        match self.cat {
            cat_deref(ref b, _, BorrowedPtr(ty::MutBorrow, _)) |
            cat_deref(ref b, _, Implicit(ty::MutBorrow, _)) |
            cat_deref(ref b, _, BorrowedPtr(ty::UniqueImmBorrow, _)) |
            cat_deref(ref b, _, Implicit(ty::UniqueImmBorrow, _)) |
            cat_downcast(ref b, _) |
            cat_deref(ref b, _, Unique) |
            cat_interior(ref b, _) => {
                // Aliasability depends on base cmt
                b.freely_aliasable(ctxt)
            }

            cat_rvalue(..) |
            cat_local(..) |
            cat_upvar(..) |
            cat_deref(_, _, UnsafePtr(..)) => { // yes, it's aliasable, but...
                None
            }

            cat_static_item(..) => {
                let int_safe = if ty::type_interior_is_unsafe(ctxt, self.ty) {
                    InteriorUnsafe
                } else {
                    InteriorSafe
                };

                if self.mutbl.is_mutable() {
                    Some(AliasableStaticMut(int_safe))
                } else {
                    Some(AliasableStatic(int_safe))
                }
            }

            cat_deref(ref base, _, BorrowedPtr(ty::ImmBorrow, _)) |
            cat_deref(ref base, _, Implicit(ty::ImmBorrow, _)) => {
                match base.cat {
                    cat_upvar(Upvar{ id, .. }) => Some(AliasableClosure(id.closure_expr_id)),
                    _ => Some(AliasableBorrowed)
                }
            }
        }
    }

    // Digs down through one or two layers of deref and grabs the cmt
    // for the upvar if a note indicates there is one.
    pub fn upvar(&self) -> Option<cmt<'tcx>> {
        match self.note {
            NoteClosureEnv(..) | NoteUpvarRef(..) => {
                Some(match self.cat {
                    cat_deref(ref inner, _, _) => {
                        match inner.cat {
                            cat_deref(ref inner, _, _) => inner.clone(),
                            cat_upvar(..) => inner.clone(),
                            _ => unreachable!()
                        }
                    }
                    _ => unreachable!()
                })
            }
            NoteNone => None
        }
    }


    pub fn descriptive_string(&self, tcx: &ty::ctxt) -> String {
        match self.cat {
            cat_static_item => {
                "static item".to_string()
            }
            cat_rvalue(..) => {
                "non-lvalue".to_string()
            }
            cat_local(vid) => {
                match tcx.map.find(vid) {
                    Some(ast_map::NodeArg(_)) => {
                        "argument".to_string()
                    }
                    _ => "local variable".to_string()
                }
            }
            cat_deref(_, _, pk) => {
                let upvar = self.upvar();
                match upvar.as_ref().map(|i| &i.cat) {
                    Some(&cat_upvar(ref var)) => {
                        var.user_string(tcx)
                    }
                    Some(_) => unreachable!(),
                    None => {
                        match pk {
                            Implicit(..) => {
                                format!("indexed content")
                            }
                            Unique => {
                                format!("`Box` content")
                            }
                            UnsafePtr(..) => {
                                format!("dereference of unsafe pointer")
                            }
                            BorrowedPtr(..) => {
                                format!("borrowed content")
                            }
                        }
                    }
                }
            }
            cat_interior(_, InteriorField(NamedField(_))) => {
                "field".to_string()
            }
            cat_interior(_, InteriorField(PositionalField(_))) => {
                "anonymous field".to_string()
            }
            cat_interior(_, InteriorElement(InteriorOffsetKind::Index,
                                            VecElement)) |
            cat_interior(_, InteriorElement(InteriorOffsetKind::Index,
                                            OtherElement)) => {
                "indexed content".to_string()
            }
            cat_interior(_, InteriorElement(InteriorOffsetKind::Pattern,
                                            VecElement)) |
            cat_interior(_, InteriorElement(InteriorOffsetKind::Pattern,
                                            OtherElement)) => {
                "pattern-bound indexed content".to_string()
            }
            cat_upvar(ref var) => {
                var.user_string(tcx)
            }
            cat_downcast(ref cmt, _) => {
                cmt.descriptive_string(tcx)
            }
        }
    }
}

impl<'tcx> Repr<'tcx> for cmt_<'tcx> {
    fn repr(&self, tcx: &ty::ctxt<'tcx>) -> String {
        format!("{{{} id:{} m:{:?} ty:{}}}",
                self.cat.repr(tcx),
                self.id,
                self.mutbl,
                self.ty.repr(tcx))
    }
}

impl<'tcx> Repr<'tcx> for categorization<'tcx> {
    fn repr(&self, tcx: &ty::ctxt<'tcx>) -> String {
        match *self {
            cat_static_item |
            cat_rvalue(..) |
            cat_local(..) |
            cat_upvar(..) => {
                format!("{:?}", *self)
            }
            cat_deref(ref cmt, derefs, ptr) => {
                format!("{}-{}{}->", cmt.cat.repr(tcx), ptr.repr(tcx), derefs)
            }
            cat_interior(ref cmt, interior) => {
                format!("{}.{}", cmt.cat.repr(tcx), interior.repr(tcx))
            }
            cat_downcast(ref cmt, _) => {
                format!("{}->(enum)", cmt.cat.repr(tcx))
            }
        }
    }
}

pub fn ptr_sigil(ptr: PointerKind) -> &'static str {
    match ptr {
        Unique => "Box",
        BorrowedPtr(ty::ImmBorrow, _) |
        Implicit(ty::ImmBorrow, _) => "&",
        BorrowedPtr(ty::MutBorrow, _) |
        Implicit(ty::MutBorrow, _) => "&mut",
        BorrowedPtr(ty::UniqueImmBorrow, _) |
        Implicit(ty::UniqueImmBorrow, _) => "&unique",
        UnsafePtr(_) => "*",
    }
}

impl<'tcx> Repr<'tcx> for PointerKind {
    fn repr(&self, tcx: &ty::ctxt<'tcx>) -> String {
        match *self {
            Unique => {
                format!("Box")
            }
            BorrowedPtr(ty::ImmBorrow, ref r) |
            Implicit(ty::ImmBorrow, ref r) => {
                format!("&{}", r.repr(tcx))
            }
            BorrowedPtr(ty::MutBorrow, ref r) |
            Implicit(ty::MutBorrow, ref r) => {
                format!("&{} mut", r.repr(tcx))
            }
            BorrowedPtr(ty::UniqueImmBorrow, ref r) |
            Implicit(ty::UniqueImmBorrow, ref r) => {
                format!("&{} uniq", r.repr(tcx))
            }
            UnsafePtr(_) => {
                format!("*")
            }
        }
    }
}

impl<'tcx> Repr<'tcx> for InteriorKind {
    fn repr(&self, _tcx: &ty::ctxt) -> String {
        match *self {
            InteriorField(NamedField(fld)) => {
                token::get_name(fld).to_string()
            }
            InteriorField(PositionalField(i)) => format!("#{}", i),
            InteriorElement(..) => "[]".to_string(),
        }
    }
}

fn element_kind(t: Ty) -> ElementKind {
    match t.sty {
        ty::ty_rptr(_, ty::mt{ty, ..}) |
        ty::ty_uniq(ty) => match ty.sty {
            ty::ty_vec(_, None) => VecElement,
            _ => OtherElement
        },
        ty::ty_vec(..) => VecElement,
        _ => OtherElement
    }
}

impl<'tcx> Repr<'tcx> for ty::ClosureKind {
    fn repr(&self, _: &ty::ctxt) -> String {
        format!("Upvar({:?})", self)
    }
}

impl<'tcx> Repr<'tcx> for Upvar {
    fn repr(&self, tcx: &ty::ctxt) -> String {
        format!("Upvar({})", self.kind.repr(tcx))
    }
}

impl<'tcx> UserString<'tcx> for Upvar {
    fn user_string(&self, _: &ty::ctxt) -> String {
        let kind = match self.kind {
            ty::FnClosureKind => "Fn",
            ty::FnMutClosureKind => "FnMut",
            ty::FnOnceClosureKind => "FnOnce",
        };
        format!("captured outer variable in an `{}` closure", kind)
    }
}
