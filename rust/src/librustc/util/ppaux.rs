// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


use metadata::encoder;
use middle::ty::{ReSkolemized, ReVar};
use middle::ty::{BoundRegion, BrAnon, BrNamed};
use middle::ty::{BrFresh, ctxt};
use middle::ty::{mt, t, param_ty};
use middle::ty::{ReFree, ReScope, ReInfer, ReStatic, Region,
                 ReEmpty};
use middle::ty::{ty_bool, ty_char, ty_bot, ty_box, ty_struct, ty_enum};
use middle::ty::{ty_err, ty_str, ty_vec, ty_float, ty_bare_fn, ty_closure};
use middle::ty::{ty_nil, ty_param, ty_ptr, ty_rptr, ty_self, ty_tup};
use middle::ty::{ty_uniq, ty_trait, ty_int, ty_uint, ty_unboxed_vec, ty_infer};
use middle::ty;
use middle::typeck;
use syntax::abi::AbiSet;
use syntax::ast_map;
use syntax::codemap::{Span, Pos};
use syntax::parse::token;
use syntax::print::pprust;
use syntax::{ast, ast_util};
use syntax::opt_vec;
use syntax::opt_vec::OptVec;

/// Produces a string suitable for debugging output.
pub trait Repr {
    fn repr(&self, tcx: ctxt) -> ~str;
}

/// Produces a string suitable for showing to the user.
pub trait UserString {
    fn user_string(&self, tcx: ctxt) -> ~str;
}

pub fn note_and_explain_region(cx: ctxt,
                               prefix: &str,
                               region: ty::Region,
                               suffix: &str) {
    match explain_region_and_span(cx, region) {
      (ref str, Some(span)) => {
        cx.sess.span_note(
            span,
            format!("{}{}{}", prefix, *str, suffix));
      }
      (ref str, None) => {
        cx.sess.note(
            format!("{}{}{}", prefix, *str, suffix));
      }
    }
}

/// Returns a string like "the block at 27:31" that attempts to explain a
/// lifetime in a way it might plausibly be understood.
pub fn explain_region(cx: ctxt, region: ty::Region) -> ~str {
  let (res, _) = explain_region_and_span(cx, region);
  return res;
}


pub fn explain_region_and_span(cx: ctxt, region: ty::Region)
                            -> (~str, Option<Span>) {
    return match region {
      ReScope(node_id) => {
        match cx.map.find(node_id) {
          Some(ast_map::NodeBlock(ref blk)) => {
            explain_span(cx, "block", blk.span)
          }
          Some(ast_map::NodeCalleeScope(expr)) => {
              explain_span(cx, "callee", expr.span)
          }
          Some(ast_map::NodeExpr(expr)) => {
            match expr.node {
              ast::ExprCall(..) => explain_span(cx, "call", expr.span),
              ast::ExprMethodCall(..) => {
                explain_span(cx, "method call", expr.span)
              },
              ast::ExprMatch(..) => explain_span(cx, "match", expr.span),
              _ => explain_span(cx, "expression", expr.span)
            }
          }
          Some(ast_map::NodeStmt(stmt)) => {
              explain_span(cx, "statement", stmt.span)
          }
          Some(ast_map::NodeItem(it)) if (match it.node {
                ast::ItemFn(..) => true, _ => false}) => {
              explain_span(cx, "function body", it.span)
          }
          Some(_) | None => {
            // this really should not happen
            (format!("unknown scope: {}.  Please report a bug.", node_id),
             None)
          }
        }
      }

      ReFree(ref fr) => {
        let prefix = match fr.bound_region {
          BrAnon(idx) => format!("the anonymous lifetime \\#{} defined on",
                               idx + 1),
          BrFresh(_) => format!("an anonymous lifetime defined on"),
          _ => format!("the lifetime {} as defined on",
                    bound_region_ptr_to_str(cx, fr.bound_region))
        };

        match cx.map.find(fr.scope_id) {
          Some(ast_map::NodeBlock(ref blk)) => {
            let (msg, opt_span) = explain_span(cx, "block", blk.span);
            (format!("{} {}", prefix, msg), opt_span)
          }
          Some(ast_map::NodeItem(it)) if match it.node {
                ast::ItemImpl(..) => true, _ => false} => {
            let (msg, opt_span) = explain_span(cx, "impl", it.span);
            (format!("{} {}", prefix, msg), opt_span)
          }
          Some(_) | None => {
            // this really should not happen
            (format!("{} node {}", prefix, fr.scope_id), None)
          }
        }
      }

      ReStatic => { (~"the static lifetime", None) }

      ReEmpty => { (~"the empty lifetime", None) }

      // I believe these cases should not occur (except when debugging,
      // perhaps)
      ty::ReInfer(_) | ty::ReEarlyBound(..) | ty::ReLateBound(..) => {
        (format!("lifetime {:?}", region), None)
      }
    };

    fn explain_span(cx: ctxt, heading: &str, span: Span)
        -> (~str, Option<Span>)
    {
        let lo = cx.sess.codemap.lookup_char_pos_adj(span.lo);
        (format!("the {} at {}:{}", heading,
              lo.line, lo.col.to_uint()), Some(span))
    }
}

pub fn bound_region_ptr_to_str(cx: ctxt, br: BoundRegion) -> ~str {
    bound_region_to_str(cx, "&", true, br)
}

pub fn bound_region_to_str(cx: ctxt,
                           prefix: &str, space: bool,
                           br: BoundRegion) -> ~str {
    let space_str = if space { " " } else { "" };

    if cx.sess.verbose() {
        return format!("{}{}{}", prefix, br.repr(cx), space_str);
    }

    match br {
        BrNamed(_, ident)   => format!("{}'{}{}", prefix,
                                       token::get_ident(ident), space_str),
        BrAnon(_)           => prefix.to_str(),
        BrFresh(_)          => prefix.to_str(),
    }
}

pub fn ReScope_id_to_str(cx: ctxt, node_id: ast::NodeId) -> ~str {
    match cx.map.find(node_id) {
      Some(ast_map::NodeBlock(ref blk)) => {
        format!("<block at {}>",
             cx.sess.codemap.span_to_str(blk.span))
      }
      Some(ast_map::NodeExpr(expr)) => {
        match expr.node {
          ast::ExprCall(..) => {
            format!("<call at {}>",
                 cx.sess.codemap.span_to_str(expr.span))
          }
          ast::ExprMatch(..) => {
            format!("<match at {}>",
                 cx.sess.codemap.span_to_str(expr.span))
          }
          ast::ExprAssignOp(..) |
          ast::ExprUnary(..) |
          ast::ExprBinary(..) |
          ast::ExprIndex(..) => {
            format!("<method at {}>",
                 cx.sess.codemap.span_to_str(expr.span))
          }
          _ => {
            format!("<expression at {}>",
                 cx.sess.codemap.span_to_str(expr.span))
          }
        }
      }
      None => {
        format!("<unknown-{}>", node_id)
      }
      _ => cx.sess.bug(format!("ReScope refers to {}", cx.map.node_to_str(node_id)))
    }
}

// In general, if you are giving a region error message,
// you should use `explain_region()` or, better yet,
// `note_and_explain_region()`
pub fn region_ptr_to_str(cx: ctxt, region: Region) -> ~str {
    region_to_str(cx, "&", true, region)
}

pub fn region_to_str(cx: ctxt, prefix: &str, space: bool, region: Region) -> ~str {
    let space_str = if space { " " } else { "" };

    if cx.sess.verbose() {
        return format!("{}{}{}", prefix, region.repr(cx), space_str);
    }

    // These printouts are concise.  They do not contain all the information
    // the user might want to diagnose an error, but there is basically no way
    // to fit that into a short string.  Hence the recommendation to use
    // `explain_region()` or `note_and_explain_region()`.
    match region {
        ty::ReScope(_) => prefix.to_str(),
        ty::ReEarlyBound(_, _, ident) => token::get_ident(ident).get().to_str(),
        ty::ReLateBound(_, br) => bound_region_to_str(cx, prefix, space, br),
        ty::ReFree(ref fr) => bound_region_to_str(cx, prefix, space, fr.bound_region),
        ty::ReInfer(ReSkolemized(_, br)) => {
            bound_region_to_str(cx, prefix, space, br)
        }
        ty::ReInfer(ReVar(_)) => prefix.to_str(),
        ty::ReStatic => format!("{}'static{}", prefix, space_str),
        ty::ReEmpty => format!("{}'<empty>{}", prefix, space_str)
    }
}

pub fn mutability_to_str(m: ast::Mutability) -> ~str {
    match m {
        ast::MutMutable => ~"mut ",
        ast::MutImmutable => ~"",
    }
}

pub fn mt_to_str(cx: ctxt, m: &mt) -> ~str {
    mt_to_str_wrapped(cx, "", m, "")
}

pub fn mt_to_str_wrapped(cx: ctxt, before: &str, m: &mt, after: &str) -> ~str {
    let mstr = mutability_to_str(m.mutbl);
    return format!("{}{}{}{}", mstr, before, ty_to_str(cx, m.ty), after);
}

pub fn vstore_to_str(cx: ctxt, vs: ty::vstore) -> ~str {
    match vs {
      ty::vstore_fixed(n) => format!("{}", n),
      ty::vstore_uniq => ~"~",
      ty::vstore_slice(r) => region_ptr_to_str(cx, r)
    }
}

pub fn trait_store_to_str(cx: ctxt, s: ty::TraitStore) -> ~str {
    match s {
      ty::UniqTraitStore => ~"~",
      ty::RegionTraitStore(r) => region_ptr_to_str(cx, r)
    }
}

pub fn vstore_ty_to_str(cx: ctxt, mt: &mt, vs: ty::vstore) -> ~str {
    match vs {
        ty::vstore_fixed(_) => {
            format!("[{}, .. {}]", mt_to_str(cx, mt), vstore_to_str(cx, vs))
        }
        _ => {
            format!("{}{}", vstore_to_str(cx, vs), mt_to_str_wrapped(cx, "[", mt, "]"))
        }
    }
}

pub fn vec_map_to_str<T>(ts: &[T], f: |t: &T| -> ~str) -> ~str {
    let tstrs = ts.map(f);
    format!("[{}]", tstrs.connect(", "))
}

pub fn tys_to_str(cx: ctxt, ts: &[t]) -> ~str {
    vec_map_to_str(ts, |t| ty_to_str(cx, *t))
}

pub fn fn_sig_to_str(cx: ctxt, typ: &ty::FnSig) -> ~str {
    format!("fn{}{} -> {}",
            typ.binder_id,
            typ.inputs.repr(cx),
            typ.output.repr(cx))
}

pub fn trait_ref_to_str(cx: ctxt, trait_ref: &ty::TraitRef) -> ~str {
    trait_ref.user_string(cx)
}

pub fn ty_to_str(cx: ctxt, typ: t) -> ~str {
    fn fn_input_to_str(cx: ctxt, input: ty::t) -> ~str {
        ty_to_str(cx, input)
    }
    fn bare_fn_to_str(cx: ctxt,
                      purity: ast::Purity,
                      abis: AbiSet,
                      ident: Option<ast::Ident>,
                      sig: &ty::FnSig)
                      -> ~str {
        let mut s = if abis.is_rust() {
            ~""
        } else {
            format!("extern {} ", abis.to_str())
        };

        match purity {
            ast::ImpureFn => {}
            _ => {
                s.push_str(purity.to_str());
                s.push_char(' ');
            }
        };

        s.push_str("fn");

        match ident {
            Some(i) => {
                s.push_char(' ');
                s.push_str(token::get_ident(i).get());
            }
            _ => { }
        }

        push_sig_to_str(cx, &mut s, '(', ')', sig);

        return s;
    }
    fn closure_to_str(cx: ctxt, cty: &ty::ClosureTy) -> ~str {
        let is_proc =
            (cty.sigil, cty.onceness) == (ast::OwnedSigil, ast::Once);
        let is_borrowed_closure = cty.sigil == ast::BorrowedSigil;

        let mut s = if is_proc || is_borrowed_closure {
            ~""
        } else {
            cty.sigil.to_str()
        };

        match (cty.sigil, cty.region) {
            (ast::ManagedSigil, ty::ReStatic) |
            (ast::OwnedSigil, ty::ReStatic) => {}

            (_, region) => {
                s.push_str(region_to_str(cx, "", true, region));
            }
        }

        match cty.purity {
            ast::ImpureFn => {}
            _ => {
                s.push_str(cty.purity.to_str());
                s.push_char(' ');
            }
        };

        if is_proc {
            s.push_str("proc");
        } else {
            match cty.onceness {
                ast::Many => {}
                ast::Once => {
                    s.push_str(cty.onceness.to_str());
                    s.push_char(' ');
                }
            };

            if !is_borrowed_closure {
                s.push_str("fn");
            }
        }

        if !is_borrowed_closure {
            // Print bounds before `fn` if this is not a borrowed closure.
            if !cty.bounds.is_empty() {
                s.push_str(":");
                s.push_str(cty.bounds.repr(cx));
            }

            push_sig_to_str(cx, &mut s, '(', ')', &cty.sig);
        } else {
            // Print bounds after the signature if this is a borrowed closure.
            push_sig_to_str(cx, &mut s, '|', '|', &cty.sig);

            if is_borrowed_closure {
                if !cty.bounds.is_empty() {
                    s.push_str(":");
                    s.push_str(cty.bounds.repr(cx));
                }
            }
        }

        return s;
    }
    fn push_sig_to_str(cx: ctxt,
                       s: &mut ~str,
                       bra: char,
                       ket: char,
                       sig: &ty::FnSig) {
        s.push_char(bra);
        let strs = sig.inputs.map(|a| fn_input_to_str(cx, *a));
        s.push_str(strs.connect(", "));
        if sig.variadic {
            s.push_str(", ...");
        }
        s.push_char(ket);

        if ty::get(sig.output).sty != ty_nil {
            s.push_str(" -> ");
            if ty::type_is_bot(sig.output) {
                s.push_char('!');
            } else {
                s.push_str(ty_to_str(cx, sig.output));
            }
        }
    }

    // if there is an id, print that instead of the structural type:
    /*for def_id in ty::type_def_id(typ).iter() {
        // note that this typedef cannot have type parameters
        return ty::item_path_str(cx, *def_id);
    }*/

    // pretty print the structural type representation:
    return match ty::get(typ).sty {
      ty_nil => ~"()",
      ty_bot => ~"!",
      ty_bool => ~"bool",
      ty_char => ~"char",
      ty_int(ast::TyI) => ~"int",
      ty_int(t) => ast_util::int_ty_to_str(t),
      ty_uint(ast::TyU) => ~"uint",
      ty_uint(t) => ast_util::uint_ty_to_str(t),
      ty_float(t) => ast_util::float_ty_to_str(t),
      ty_box(typ) => ~"@" + ty_to_str(cx, typ),
      ty_uniq(typ) => ~"~" + ty_to_str(cx, typ),
      ty_ptr(ref tm) => ~"*" + mt_to_str(cx, tm),
      ty_rptr(r, ref tm) => {
        region_ptr_to_str(cx, r) + mt_to_str(cx, tm)
      }
      ty_unboxed_vec(ref tm) => { format!("unboxed_vec<{}>", mt_to_str(cx, tm)) }
      ty_tup(ref elems) => {
        let strs = elems.map(|elem| ty_to_str(cx, *elem));
        ~"(" + strs.connect(",") + ")"
      }
      ty_closure(ref f) => {
          closure_to_str(cx, f)
      }
      ty_bare_fn(ref f) => {
          bare_fn_to_str(cx, f.purity, f.abis, None, &f.sig)
      }
      ty_infer(infer_ty) => infer_ty.to_str(),
      ty_err => ~"[type error]",
      ty_param(param_ty {idx: id, def_id: did}) => {
          let ty_param_defs = cx.ty_param_defs.borrow();
          let param_def = ty_param_defs.get().find(&did.node);
          let ident = match param_def {
              Some(def) => token::get_ident(def.ident).get().to_str(),
              // This should not happen...
              None => format!("BUG[{:?}]", id)
          };
          if !cx.sess.verbose() {
              ident
          } else {
            format!("{}:{:?}", ident, did)
          }
      }
      ty_self(..) => ~"Self",
      ty_enum(did, ref substs) | ty_struct(did, ref substs) => {
        let base = ty::item_path_str(cx, did);
        parameterized(cx, base, &substs.regions, substs.tps, did, false)
      }
      ty_trait(did, ref substs, s, mutbl, ref bounds) => {
        let base = ty::item_path_str(cx, did);
        let ty = parameterized(cx, base, &substs.regions,
                               substs.tps, did, true);
        let bound_sep = if bounds.is_empty() { "" } else { ":" };
        let bound_str = bounds.repr(cx);
        format!("{}{}{}{}{}", trait_store_to_str(cx, s), mutability_to_str(mutbl), ty,
                           bound_sep, bound_str)
      }
      ty_vec(ref mt, vs) => {
        vstore_ty_to_str(cx, mt, vs)
      }
      ty_str(vs) => format!("{}{}", vstore_to_str(cx, vs), "str")
    }
}

pub fn parameterized(cx: ctxt,
                     base: &str,
                     regions: &ty::RegionSubsts,
                     tps: &[ty::t],
                     did: ast::DefId,
                     is_trait: bool) -> ~str {

    let mut strs = ~[];
    match *regions {
        ty::ErasedRegions => { }
        ty::NonerasedRegions(ref regions) => {
            for &r in regions.iter() {
                strs.push(region_to_str(cx, "", false, r))
            }
        }
    }

    let generics = if is_trait {
        ty::lookup_trait_def(cx, did).generics.clone()
    } else {
        ty::lookup_item_type(cx, did).generics
    };
    let ty_params = generics.type_param_defs();
    let has_defaults = ty_params.last().map_or(false, |def| def.default.is_some());
    let num_defaults = if has_defaults {
        // We should have a borrowed version of substs instead of cloning.
        let mut substs = ty::substs {
            tps: tps.to_owned(),
            regions: regions.clone(),
            self_ty: None
        };
        ty_params.iter().zip(tps.iter()).rev().take_while(|&(def, &actual)| {
            substs.tps.pop();
            match def.default {
                Some(default) => ty::subst(cx, &substs, default) == actual,
                None => false
            }
        }).len()
    } else {
        0
    };

    for t in tps.slice_to(tps.len() - num_defaults).iter() {
        strs.push(ty_to_str(cx, *t))
    }

    if strs.len() > 0u {
        format!("{}<{}>", base, strs.connect(","))
    } else {
        format!("{}", base)
    }
}

pub fn ty_to_short_str(cx: ctxt, typ: t) -> ~str {
    let mut s = encoder::encoded_ty(cx, typ);
    if s.len() >= 32u { s = s.slice(0u, 32u).to_owned(); }
    return s;
}

impl<T:Repr> Repr for Option<T> {
    fn repr(&self, tcx: ctxt) -> ~str {
        match self {
            &None => ~"None",
            &Some(ref t) => t.repr(tcx),
        }
    }
}

impl<T:Repr,U:Repr> Repr for Result<T,U> {
    fn repr(&self, tcx: ctxt) -> ~str {
        match self {
            &Ok(ref t) => t.repr(tcx),
            &Err(ref u) => format!("Err({})", u.repr(tcx))
        }
    }
}

impl Repr for () {
    fn repr(&self, _tcx: ctxt) -> ~str {
        ~"()"
    }
}

impl<T:Repr> Repr for @T {
    fn repr(&self, tcx: ctxt) -> ~str {
        (&**self).repr(tcx)
    }
}

impl<T:Repr> Repr for ~T {
    fn repr(&self, tcx: ctxt) -> ~str {
        (&**self).repr(tcx)
    }
}

fn repr_vec<T:Repr>(tcx: ctxt, v: &[T]) -> ~str {
    vec_map_to_str(v, |t| t.repr(tcx))
}

impl<'a, T:Repr> Repr for &'a [T] {
    fn repr(&self, tcx: ctxt) -> ~str {
        repr_vec(tcx, *self)
    }
}

impl<T:Repr> Repr for OptVec<T> {
    fn repr(&self, tcx: ctxt) -> ~str {
        match *self {
            opt_vec::Empty => ~"[]",
            opt_vec::Vec(ref v) => repr_vec(tcx, *v)
        }
    }
}

// This is necessary to handle types like Option<~[T]>, for which
// autoderef cannot convert the &[T] handler
impl<T:Repr> Repr for ~[T] {
    fn repr(&self, tcx: ctxt) -> ~str {
        repr_vec(tcx, *self)
    }
}

impl Repr for ty::TypeParameterDef {
    fn repr(&self, tcx: ctxt) -> ~str {
        format!("TypeParameterDef({:?}, {})",
                self.def_id,
                self.bounds.repr(tcx))
    }
}

impl Repr for ty::RegionParameterDef {
    fn repr(&self, _tcx: ctxt) -> ~str {
        format!("RegionParameterDef({}, {:?})",
                token::get_ident(self.ident),
                self.def_id)
    }
}

impl Repr for ty::t {
    fn repr(&self, tcx: ctxt) -> ~str {
        ty_to_str(tcx, *self)
    }
}

impl Repr for ty::substs {
    fn repr(&self, tcx: ctxt) -> ~str {
        format!("substs(regions={}, self_ty={}, tps={})",
             self.regions.repr(tcx),
             self.self_ty.repr(tcx),
             self.tps.repr(tcx))
    }
}

impl Repr for ty::RegionSubsts {
    fn repr(&self, tcx: ctxt) -> ~str {
        match *self {
            ty::ErasedRegions => ~"erased",
            ty::NonerasedRegions(ref regions) => regions.repr(tcx)
        }
    }
}

impl Repr for ty::ParamBounds {
    fn repr(&self, tcx: ctxt) -> ~str {
        let mut res = ~[];
        for b in self.builtin_bounds.iter() {
            res.push(match b {
                ty::BoundStatic => ~"'static",
                ty::BoundSend => ~"Send",
                ty::BoundFreeze => ~"Freeze",
                ty::BoundSized => ~"Sized",
                ty::BoundPod => ~"Pod",
            });
        }
        for t in self.trait_bounds.iter() {
            res.push(t.repr(tcx));
        }
        res.connect("+")
    }
}

impl Repr for ty::TraitRef {
    fn repr(&self, tcx: ctxt) -> ~str {
        trait_ref_to_str(tcx, self)
    }
}

impl Repr for ast::Expr {
    fn repr(&self, _tcx: ctxt) -> ~str {
        format!("expr({}: {})", self.id, pprust::expr_to_str(self))
    }
}

impl Repr for ast::Item {
    fn repr(&self, tcx: ctxt) -> ~str {
        format!("item({})", tcx.map.node_to_str(self.id))
    }
}

impl Repr for ast::Stmt {
    fn repr(&self, _tcx: ctxt) -> ~str {
        format!("stmt({}: {})",
                ast_util::stmt_id(self),
                pprust::stmt_to_str(self))
    }
}

impl Repr for ast::Pat {
    fn repr(&self, _tcx: ctxt) -> ~str {
        format!("pat({}: {})",
             self.id,
             pprust::pat_to_str(self))
    }
}

impl Repr for ty::BoundRegion {
    fn repr(&self, tcx: ctxt) -> ~str {
        match *self {
            ty::BrAnon(id) => format!("BrAnon({})", id),
            ty::BrNamed(id, ident) => format!("BrNamed({}, {})",
                                               id.repr(tcx),
                                               ident.repr(tcx)),
            ty::BrFresh(id) => format!("BrFresh({})", id),
        }
    }
}

impl Repr for ty::Region {
    fn repr(&self, tcx: ctxt) -> ~str {
        match *self {
            ty::ReEarlyBound(id, index, ident) => {
                format!("ReEarlyBound({}, {}, {})",
                        id, index, ident.repr(tcx))
            }

            ty::ReLateBound(binder_id, ref bound_region) => {
                format!("ReLateBound({}, {})",
                        binder_id, bound_region.repr(tcx))
            }

            ty::ReFree(ref fr) => {
                format!("ReFree({}, {})",
                        fr.scope_id,
                        fr.bound_region.repr(tcx))
            }

            ty::ReScope(id) => {
                format!("ReScope({})", id)
            }

            ty::ReStatic => {
                format!("ReStatic")
            }

            ty::ReInfer(ReVar(ref vid)) => {
                format!("ReInfer({})", vid.id)
            }

            ty::ReInfer(ReSkolemized(id, ref bound_region)) => {
                format!("re_skolemized({}, {})",
                        id, bound_region.repr(tcx))
            }

            ty::ReEmpty => {
                format!("ReEmpty")
            }
        }
    }
}

impl Repr for ast::DefId {
    fn repr(&self, tcx: ctxt) -> ~str {
        // Unfortunately, there seems to be no way to attempt to print
        // a path for a def-id, so I'll just make a best effort for now
        // and otherwise fallback to just printing the crate/node pair
        if self.krate == ast::LOCAL_CRATE {
            {
                match tcx.map.find(self.node) {
                    Some(ast_map::NodeItem(..)) |
                    Some(ast_map::NodeForeignItem(..)) |
                    Some(ast_map::NodeMethod(..)) |
                    Some(ast_map::NodeTraitMethod(..)) |
                    Some(ast_map::NodeVariant(..)) |
                    Some(ast_map::NodeStructCtor(..)) => {
                        return format!("{:?}:{}",
                                       *self,
                                       ty::item_path_str(tcx, *self));
                    }
                    _ => {}
                }
            }
        }
        return format!("{:?}", *self);
    }
}

impl Repr for ty::ty_param_bounds_and_ty {
    fn repr(&self, tcx: ctxt) -> ~str {
        format!("ty_param_bounds_and_ty \\{generics: {}, ty: {}\\}",
             self.generics.repr(tcx),
             self.ty.repr(tcx))
    }
}

impl Repr for ty::Generics {
    fn repr(&self, tcx: ctxt) -> ~str {
        format!("Generics(type_param_defs: {}, region_param_defs: {})",
                self.type_param_defs().repr(tcx),
                self.region_param_defs().repr(tcx))
    }
}

impl Repr for ty::ItemVariances {
    fn repr(&self, tcx: ctxt) -> ~str {
        format!("IterVariances(self_param={}, type_params={}, region_params={})",
                self.self_param.repr(tcx),
                self.type_params.repr(tcx),
                self.region_params.repr(tcx))
    }
}

impl Repr for ty::Variance {
    fn repr(&self, _: ctxt) -> ~str {
        self.to_str().to_owned()
    }
}

impl Repr for ty::Method {
    fn repr(&self, tcx: ctxt) -> ~str {
        format!("method(ident: {}, generics: {}, fty: {}, \
                explicit_self: {}, vis: {}, def_id: {})",
                self.ident.repr(tcx),
                self.generics.repr(tcx),
                self.fty.repr(tcx),
                self.explicit_self.repr(tcx),
                self.vis.repr(tcx),
                self.def_id.repr(tcx))
    }
}

impl Repr for ast::Ident {
    fn repr(&self, _tcx: ctxt) -> ~str {
        token::get_ident(*self).get().to_str()
    }
}

impl Repr for ast::ExplicitSelf_ {
    fn repr(&self, _tcx: ctxt) -> ~str {
        format!("{:?}", *self)
    }
}

impl Repr for ast::Visibility {
    fn repr(&self, _tcx: ctxt) -> ~str {
        format!("{:?}", *self)
    }
}

impl Repr for ty::BareFnTy {
    fn repr(&self, tcx: ctxt) -> ~str {
        format!("BareFnTy \\{purity: {:?}, abis: {}, sig: {}\\}",
             self.purity,
             self.abis.to_str(),
             self.sig.repr(tcx))
    }
}

impl Repr for ty::FnSig {
    fn repr(&self, tcx: ctxt) -> ~str {
        fn_sig_to_str(tcx, self)
    }
}

impl Repr for typeck::method_origin {
    fn repr(&self, tcx: ctxt) -> ~str {
        match self {
            &typeck::method_static(def_id) => {
                format!("method_static({})", def_id.repr(tcx))
            }
            &typeck::method_param(ref p) => {
                p.repr(tcx)
            }
            &typeck::method_object(ref p) => {
                p.repr(tcx)
            }
        }
    }
}

impl Repr for typeck::method_param {
    fn repr(&self, tcx: ctxt) -> ~str {
        format!("method_param({},{:?},{:?},{:?})",
             self.trait_id.repr(tcx),
             self.method_num,
             self.param_num,
             self.bound_num)
    }
}

impl Repr for typeck::method_object {
    fn repr(&self, tcx: ctxt) -> ~str {
        format!("method_object({},{:?},{:?})",
             self.trait_id.repr(tcx),
             self.method_num,
             self.real_index)
    }
}


impl Repr for ty::RegionVid {
    fn repr(&self, _tcx: ctxt) -> ~str {
        format!("{:?}", *self)
    }
}

impl Repr for ty::TraitStore {
    fn repr(&self, tcx: ctxt) -> ~str {
        match self {
            &ty::UniqTraitStore => ~"~Trait",
            &ty::RegionTraitStore(r) => format!("&{} Trait", r.repr(tcx))
        }
    }
}

impl Repr for ty::vstore {
    fn repr(&self, tcx: ctxt) -> ~str {
        vstore_to_str(tcx, *self)
    }
}

impl Repr for ty::BuiltinBound {
    fn repr(&self, _tcx: ctxt) -> ~str {
        format!("{:?}", *self)
    }
}

impl UserString for ty::BuiltinBound {
    fn user_string(&self, _tcx: ctxt) -> ~str {
        match *self {
            ty::BoundStatic => ~"'static",
            ty::BoundSend => ~"Send",
            ty::BoundFreeze => ~"Freeze",
            ty::BoundSized => ~"Sized",
            ty::BoundPod => ~"Pod",
        }
    }
}

impl Repr for ty::BuiltinBounds {
    fn repr(&self, tcx: ctxt) -> ~str {
        self.user_string(tcx)
    }
}

impl Repr for Span {
    fn repr(&self, tcx: ctxt) -> ~str {
        tcx.sess.codemap.span_to_str(*self)
    }
}

impl<A:UserString> UserString for @A {
    fn user_string(&self, tcx: ctxt) -> ~str {
        let this: &A = &**self;
        this.user_string(tcx)
    }
}

impl UserString for ty::BuiltinBounds {
    fn user_string(&self, tcx: ctxt) -> ~str {
        if self.is_empty() { ~"<no-bounds>" } else {
            let mut result = ~[];
            for bb in self.iter() {
                result.push(bb.user_string(tcx));
            }
            result.connect("+")
        }
    }
}

impl UserString for ty::TraitRef {
    fn user_string(&self, tcx: ctxt) -> ~str {
        let base = ty::item_path_str(tcx, self.def_id);
        if tcx.sess.verbose() && self.substs.self_ty.is_some() {
            let mut all_tps = self.substs.tps.clone();
            for &t in self.substs.self_ty.iter() { all_tps.push(t); }
            parameterized(tcx, base, &self.substs.regions,
                          all_tps, self.def_id, true)
        } else {
            parameterized(tcx, base, &self.substs.regions,
                          self.substs.tps, self.def_id, true)
        }
    }
}

impl UserString for ty::t {
    fn user_string(&self, tcx: ctxt) -> ~str {
        ty_to_str(tcx, *self)
    }
}

impl Repr for AbiSet {
    fn repr(&self, _tcx: ctxt) -> ~str {
        self.to_str()
    }
}

impl UserString for AbiSet {
    fn user_string(&self, _tcx: ctxt) -> ~str {
        self.to_str()
    }
}

impl Repr for ty::UpvarId {
    fn repr(&self, tcx: ctxt) -> ~str {
        format!("UpvarId({};`{}`;{})",
             self.var_id,
             ty::local_var_name_str(tcx, self.var_id),
             self.closure_expr_id)
    }
}

impl Repr for ast::Mutability {
    fn repr(&self, _tcx: ctxt) -> ~str {
        format!("{:?}", *self)
    }
}

impl Repr for ty::BorrowKind {
    fn repr(&self, _tcx: ctxt) -> ~str {
        format!("{:?}", *self)
    }
}

impl Repr for ty::UpvarBorrow {
    fn repr(&self, tcx: ctxt) -> ~str {
        format!("UpvarBorrow({}, {})",
             self.kind.repr(tcx),
             self.region.repr(tcx))
    }
}
