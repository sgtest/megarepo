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
use middle::ty::{bound_region, br_anon, br_named, br_self, br_cap_avoid};
use middle::ty::{br_fresh, ctxt, field};
use middle::ty::{mt, t, param_ty};
use middle::ty::{re_bound, re_free, re_scope, re_infer, re_static, Region,
                 re_empty};
use middle::ty::{ty_bool, ty_char, ty_bot, ty_box, ty_struct, ty_enum};
use middle::ty::{ty_err, ty_estr, ty_evec, ty_float, ty_bare_fn, ty_closure};
use middle::ty::{ty_nil, ty_opaque_box, ty_opaque_closure_ptr, ty_param};
use middle::ty::{ty_ptr, ty_rptr, ty_self, ty_tup, ty_type, ty_uniq};
use middle::ty::{ty_trait, ty_int};
use middle::ty::{ty_uint, ty_unboxed_vec, ty_infer};
use middle::ty;
use middle::typeck;
use syntax::abi::AbiSet;
use syntax::ast_map;
use syntax::codemap::Span;
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
            fmt!("%s%s%s", prefix, (*str), suffix));
      }
      (ref str, None) => {
        cx.sess.note(
            fmt!("%s%s%s", prefix, (*str), suffix));
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
      re_scope(node_id) => {
        match cx.items.find(&node_id) {
          Some(&ast_map::node_block(ref blk)) => {
            explain_span(cx, "block", blk.span)
          }
          Some(&ast_map::node_callee_scope(expr)) => {
              explain_span(cx, "callee", expr.span)
          }
          Some(&ast_map::node_expr(expr)) => {
            match expr.node {
              ast::ExprCall(*) => explain_span(cx, "call", expr.span),
              ast::ExprMethodCall(*) => {
                explain_span(cx, "method call", expr.span)
              },
              ast::ExprMatch(*) => explain_span(cx, "match", expr.span),
              _ => explain_span(cx, "expression", expr.span)
            }
          }
          Some(&ast_map::node_stmt(stmt)) => {
              explain_span(cx, "statement", stmt.span)
          }
          Some(&ast_map::node_item(it, _)) if (match it.node {
                ast::item_fn(*) => true, _ => false}) => {
              explain_span(cx, "function body", it.span)
          }
          Some(_) | None => {
            // this really should not happen
            (fmt!("unknown scope: %d.  Please report a bug.", node_id),
             None)
          }
        }
      }

      re_free(ref fr) => {
        let prefix = match fr.bound_region {
          br_anon(idx) => fmt!("the anonymous lifetime #%u defined on",
                               idx + 1),
          br_fresh(_) => fmt!("an anonymous lifetime defined on"),
          _ => fmt!("the lifetime %s as defined on",
                    bound_region_ptr_to_str(cx, fr.bound_region))
        };

        match cx.items.find(&fr.scope_id) {
          Some(&ast_map::node_block(ref blk)) => {
            let (msg, opt_span) = explain_span(cx, "block", blk.span);
            (fmt!("%s %s", prefix, msg), opt_span)
          }
          Some(_) | None => {
            // this really should not happen
            (fmt!("%s node %d", prefix, fr.scope_id), None)
          }
        }
      }

      re_static => { (~"the static lifetime", None) }

      re_empty => { (~"the empty lifetime", None) }

      // I believe these cases should not occur (except when debugging,
      // perhaps)
      re_infer(_) | re_bound(_) => {
        (fmt!("lifetime %?", region), None)
      }
    };

    fn explain_span(cx: ctxt, heading: &str, span: Span)
        -> (~str, Option<Span>)
    {
        let lo = cx.sess.codemap.lookup_char_pos_adj(span.lo);
        (fmt!("the %s at %u:%u", heading,
              lo.line, lo.col.to_uint()), Some(span))
    }
}

pub fn bound_region_ptr_to_str(cx: ctxt, br: bound_region) -> ~str {
    bound_region_to_str(cx, "&", true, br)
}

pub fn bound_region_to_str(cx: ctxt,
                           prefix: &str, space: bool,
                           br: bound_region) -> ~str {
    let space_str = if space { " " } else { "" };

    if cx.sess.verbose() { return fmt!("%s%?%s", prefix, br, space_str); }

    match br {
      br_named(id)         => fmt!("%s'%s%s", prefix, cx.sess.str_of(id), space_str),
      br_self              => fmt!("%s'self%s", prefix, space_str),
      br_anon(_)           => prefix.to_str(),
      br_fresh(_)          => prefix.to_str(),
      br_cap_avoid(_, br)  => bound_region_to_str(cx, prefix, space, *br)
    }
}

pub fn re_scope_id_to_str(cx: ctxt, node_id: ast::NodeId) -> ~str {
    match cx.items.find(&node_id) {
      Some(&ast_map::node_block(ref blk)) => {
        fmt!("<block at %s>",
             cx.sess.codemap.span_to_str(blk.span))
      }
      Some(&ast_map::node_expr(expr)) => {
        match expr.node {
          ast::ExprCall(*) => {
            fmt!("<call at %s>",
                 cx.sess.codemap.span_to_str(expr.span))
          }
          ast::ExprMatch(*) => {
            fmt!("<match at %s>",
                 cx.sess.codemap.span_to_str(expr.span))
          }
          ast::ExprAssignOp(*) |
          ast::ExprUnary(*) |
          ast::ExprBinary(*) |
          ast::ExprIndex(*) => {
            fmt!("<method at %s>",
                 cx.sess.codemap.span_to_str(expr.span))
          }
          _ => {
            fmt!("<expression at %s>",
                 cx.sess.codemap.span_to_str(expr.span))
          }
        }
      }
      None => {
        fmt!("<unknown-%d>", node_id)
      }
      _ => { cx.sess.bug(
          fmt!("re_scope refers to %s",
               ast_map::node_id_to_str(cx.items, node_id,
                                       token::get_ident_interner()))) }
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
        return fmt!("%s%?%s", prefix, region, space_str);
    }

    // These printouts are concise.  They do not contain all the information
    // the user might want to diagnose an error, but there is basically no way
    // to fit that into a short string.  Hence the recommendation to use
    // `explain_region()` or `note_and_explain_region()`.
    match region {
        re_scope(_) => prefix.to_str(),
        re_bound(br) => bound_region_to_str(cx, prefix, space, br),
        re_free(ref fr) => bound_region_to_str(cx, prefix, space, fr.bound_region),
        re_infer(ReSkolemized(_, br)) => {
            bound_region_to_str(cx, prefix, space, br)
        }
        re_infer(ReVar(_)) => prefix.to_str(),
        re_static => fmt!("%s'static%s", prefix, space_str),
        re_empty => fmt!("%s'<empty>%s", prefix, space_str)
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
    return fmt!("%s%s%s%s", mstr, before, ty_to_str(cx, m.ty), after);
}

pub fn vstore_to_str(cx: ctxt, vs: ty::vstore) -> ~str {
    match vs {
      ty::vstore_fixed(n) => fmt!("%u", n),
      ty::vstore_uniq => ~"~",
      ty::vstore_box => ~"@",
      ty::vstore_slice(r) => region_ptr_to_str(cx, r)
    }
}

pub fn trait_store_to_str(cx: ctxt, s: ty::TraitStore) -> ~str {
    match s {
      ty::UniqTraitStore => ~"~",
      ty::BoxTraitStore => ~"@",
      ty::RegionTraitStore(r) => region_ptr_to_str(cx, r)
    }
}

pub fn vstore_ty_to_str(cx: ctxt, mt: &mt, vs: ty::vstore) -> ~str {
    match vs {
        ty::vstore_fixed(_) => {
            fmt!("[%s, .. %s]", mt_to_str(cx, mt), vstore_to_str(cx, vs))
        }
        _ => {
            fmt!("%s%s", vstore_to_str(cx, vs), mt_to_str_wrapped(cx, "[", mt, "]"))
        }
    }
}

pub fn vec_map_to_str<T>(ts: &[T], f: &fn(t: &T) -> ~str) -> ~str {
    let tstrs = ts.map(f);
    fmt!("[%s]", tstrs.connect(", "))
}

pub fn tys_to_str(cx: ctxt, ts: &[t]) -> ~str {
    vec_map_to_str(ts, |t| ty_to_str(cx, *t))
}

pub fn fn_sig_to_str(cx: ctxt, typ: &ty::FnSig) -> ~str {
    fmt!("fn%s -> %s",
         tys_to_str(cx, typ.inputs.map(|a| *a)),
         ty_to_str(cx, typ.output))
}

pub fn trait_ref_to_str(cx: ctxt, trait_ref: &ty::TraitRef) -> ~str {
    trait_ref.user_string(cx)
}

pub fn ty_to_str(cx: ctxt, typ: t) -> ~str {
    fn fn_input_to_str(cx: ctxt, input: ty::t) -> ~str {
        ty_to_str(cx, input)
    }
    fn bare_fn_to_str(cx: ctxt,
                      purity: ast::purity,
                      abis: AbiSet,
                      ident: Option<ast::Ident>,
                      sig: &ty::FnSig)
                      -> ~str {
        let mut s = ~"extern ";

        s.push_str(abis.to_str());
        s.push_char(' ');

        match purity {
            ast::impure_fn => {}
            _ => {
                s.push_str(purity.to_str());
                s.push_char(' ');
            }
        };

        s.push_str("fn");

        match ident {
          Some(i) => {
              s.push_char(' ');
              s.push_str(cx.sess.str_of(i));
          }
          _ => { }
        }

        push_sig_to_str(cx, &mut s, sig);

        return s;
    }
    fn closure_to_str(cx: ctxt, cty: &ty::ClosureTy) -> ~str
    {
        let mut s = cty.sigil.to_str();

        match (cty.sigil, cty.region) {
            (ast::ManagedSigil, ty::re_static) |
            (ast::OwnedSigil, ty::re_static) => {}

            (_, region) => {
                s.push_str(region_to_str(cx, "", true, region));
            }
        }

        match cty.purity {
            ast::impure_fn => {}
            _ => {
                s.push_str(cty.purity.to_str());
                s.push_char(' ');
            }
        };

        match cty.onceness {
            ast::Many => {}
            ast::Once => {
                s.push_str(cty.onceness.to_str());
                s.push_char(' ');
            }
        };

        s.push_str("fn");

        if !cty.bounds.is_empty() {
            s.push_str(":");
        }
        s.push_str(cty.bounds.repr(cx));

        push_sig_to_str(cx, &mut s, &cty.sig);

        return s;
    }
    fn push_sig_to_str(cx: ctxt, s: &mut ~str, sig: &ty::FnSig) {
        s.push_char('(');
        let strs = sig.inputs.map(|a| fn_input_to_str(cx, *a));
        s.push_str(strs.connect(", "));
        s.push_char(')');
        if ty::get(sig.output).sty != ty_nil {
            s.push_str(" -> ");
            if ty::type_is_bot(sig.output) {
                s.push_char('!');
            } else {
                s.push_str(ty_to_str(cx, sig.output));
            }
        }
    }
    fn method_to_str(cx: ctxt, m: ty::Method) -> ~str {
        bare_fn_to_str(cx,
                       m.fty.purity,
                       m.fty.abis,
                       Some(m.ident),
                       &m.fty.sig) + ";"
    }
    fn field_to_str(cx: ctxt, f: field) -> ~str {
        return fmt!("%s: %s", cx.sess.str_of(f.ident), mt_to_str(cx, &f.mt));
    }

    // if there is an id, print that instead of the structural type:
    /*for def_id in ty::type_def_id(typ).iter() {
        // note that this typedef cannot have type parameters
        return ast_map::path_to_str(ty::item_path(cx, *def_id),
                                    cx.sess.intr());
    }*/

    // pretty print the structural type representation:
    return match ty::get(typ).sty {
      ty_nil => ~"()",
      ty_bot => ~"!",
      ty_bool => ~"bool",
      ty_char => ~"char",
      ty_int(ast::ty_i) => ~"int",
      ty_int(t) => ast_util::int_ty_to_str(t),
      ty_uint(ast::ty_u) => ~"uint",
      ty_uint(t) => ast_util::uint_ty_to_str(t),
      ty_float(ast::ty_f) => ~"float",
      ty_float(t) => ast_util::float_ty_to_str(t),
      ty_box(ref tm) => ~"@" + mt_to_str(cx, tm),
      ty_uniq(ref tm) => ~"~" + mt_to_str(cx, tm),
      ty_ptr(ref tm) => ~"*" + mt_to_str(cx, tm),
      ty_rptr(r, ref tm) => {
        region_ptr_to_str(cx, r) + mt_to_str(cx, tm)
      }
      ty_unboxed_vec(ref tm) => { fmt!("unboxed_vec<%s>", mt_to_str(cx, tm)) }
      ty_type => ~"type",
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
          let param_def = cx.ty_param_defs.find(&did.node);
          let ident = match param_def {
              Some(def) => {
                  cx.sess.str_of(def.ident).to_owned()
              }
              None => {
                  // This should not happen...
                  fmt!("BUG[%?]", id)
              }
          };
          if !cx.sess.verbose() { ident } else { fmt!("%s:%?", ident, did) }
      }
      ty_self(*) => ~"Self",
      ty_enum(did, ref substs) | ty_struct(did, ref substs) => {
        let path = ty::item_path(cx, did);
        let base = ast_map::path_to_str(path, cx.sess.intr());
        parameterized(cx, base, &substs.regions, substs.tps)
      }
      ty_trait(did, ref substs, s, mutbl, ref bounds) => {
        let path = ty::item_path(cx, did);
        let base = ast_map::path_to_str(path, cx.sess.intr());
        let ty = parameterized(cx, base, &substs.regions, substs.tps);
        let bound_sep = if bounds.is_empty() { "" } else { ":" };
        let bound_str = bounds.repr(cx);
        fmt!("%s%s%s%s%s", trait_store_to_str(cx, s), mutability_to_str(mutbl), ty,
                           bound_sep, bound_str)
      }
      ty_evec(ref mt, vs) => {
        vstore_ty_to_str(cx, mt, vs)
      }
      ty_estr(vs) => fmt!("%s%s", vstore_to_str(cx, vs), "str"),
      ty_opaque_box => ~"@?",
      ty_opaque_closure_ptr(ast::BorrowedSigil) => ~"&closure",
      ty_opaque_closure_ptr(ast::ManagedSigil) => ~"@closure",
      ty_opaque_closure_ptr(ast::OwnedSigil) => ~"~closure",
    }
}

pub fn parameterized(cx: ctxt,
                     base: &str,
                     regions: &ty::RegionSubsts,
                     tps: &[ty::t]) -> ~str {

    let mut strs = ~[];
    match *regions {
        ty::ErasedRegions => { }
        ty::NonerasedRegions(ref regions) => {
            for &r in regions.iter() {
                strs.push(region_to_str(cx, "", false, r))
            }
        }
    }

    for t in tps.iter() {
        strs.push(ty_to_str(cx, *t))
    }

    if strs.len() > 0u {
        fmt!("%s<%s>", base, strs.connect(","))
    } else {
        fmt!("%s", base)
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
            &Some(ref t) => fmt!("Some(%s)", t.repr(tcx))
        }
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

impl<'self, T:Repr> Repr for &'self [T] {
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
        fmt!("TypeParameterDef {%?, bounds: %s}",
             self.def_id, self.bounds.repr(tcx))
    }
}

impl Repr for ty::t {
    fn repr(&self, tcx: ctxt) -> ~str {
        ty_to_str(tcx, *self)
    }
}

impl Repr for ty::substs {
    fn repr(&self, tcx: ctxt) -> ~str {
        fmt!("substs(regions=%s, self_ty=%s, tps=%s)",
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
    fn repr(&self, tcx: ctxt) -> ~str {
        fmt!("expr(%d: %s)",
             self.id,
             pprust::expr_to_str(self, tcx.sess.intr()))
    }
}

impl Repr for ast::Pat {
    fn repr(&self, tcx: ctxt) -> ~str {
        fmt!("pat(%d: %s)",
             self.id,
             pprust::pat_to_str(self, tcx.sess.intr()))
    }
}

impl Repr for ty::bound_region {
    fn repr(&self, tcx: ctxt) -> ~str {
        bound_region_ptr_to_str(tcx, *self)
    }
}

impl Repr for ty::Region {
    fn repr(&self, tcx: ctxt) -> ~str {
        region_to_str(tcx, "", false, *self)
    }
}

impl Repr for ast::DefId {
    fn repr(&self, tcx: ctxt) -> ~str {
        // Unfortunately, there seems to be no way to attempt to print
        // a path for a def-id, so I'll just make a best effort for now
        // and otherwise fallback to just printing the crate/node pair
        if self.crate == ast::LOCAL_CRATE {
            match tcx.items.find(&self.node) {
                Some(&ast_map::node_item(*)) |
                Some(&ast_map::node_foreign_item(*)) |
                Some(&ast_map::node_method(*)) |
                Some(&ast_map::node_trait_method(*)) |
                Some(&ast_map::node_variant(*)) |
                Some(&ast_map::node_struct_ctor(*)) => {
                    return fmt!("%?:%s", *self, ty::item_path_str(tcx, *self));
                }
                _ => {}
            }
        }
        return fmt!("%?", *self);
    }
}

impl Repr for ty::ty_param_bounds_and_ty {
    fn repr(&self, tcx: ctxt) -> ~str {
        fmt!("ty_param_bounds_and_ty {generics: %s, ty: %s}",
             self.generics.repr(tcx),
             self.ty.repr(tcx))
    }
}

impl Repr for ty::Generics {
    fn repr(&self, tcx: ctxt) -> ~str {
        fmt!("Generics {type_param_defs: %s, region_param: %?}",
             self.type_param_defs.repr(tcx),
             self.region_param)
    }
}

impl Repr for ty::Method {
    fn repr(&self, tcx: ctxt) -> ~str {
        fmt!("method {ident: %s, generics: %s, transformed_self_ty: %s, \
              fty: %s, explicit_self: %s, vis: %s, def_id: %s}",
             self.ident.repr(tcx),
             self.generics.repr(tcx),
             self.transformed_self_ty.repr(tcx),
             self.fty.repr(tcx),
             self.explicit_self.repr(tcx),
             self.vis.repr(tcx),
             self.def_id.repr(tcx))
    }
}

impl Repr for ast::Ident {
    fn repr(&self, _tcx: ctxt) -> ~str {
        token::ident_to_str(self).to_owned()
    }
}

impl Repr for ast::explicit_self_ {
    fn repr(&self, _tcx: ctxt) -> ~str {
        fmt!("%?", *self)
    }
}

impl Repr for ast::visibility {
    fn repr(&self, _tcx: ctxt) -> ~str {
        fmt!("%?", *self)
    }
}

impl Repr for ty::BareFnTy {
    fn repr(&self, tcx: ctxt) -> ~str {
        fmt!("BareFnTy {purity: %?, abis: %s, sig: %s}",
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

impl Repr for typeck::method_map_entry {
    fn repr(&self, tcx: ctxt) -> ~str {
        fmt!("method_map_entry {self_arg: %s, \
              explicit_self: %s, \
              origin: %s}",
             self.self_ty.repr(tcx),
             self.explicit_self.repr(tcx),
             self.origin.repr(tcx))
    }
}

impl Repr for typeck::method_origin {
    fn repr(&self, tcx: ctxt) -> ~str {
        match self {
            &typeck::method_static(def_id) => {
                fmt!("method_static(%s)", def_id.repr(tcx))
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
        fmt!("method_param(%s,%?,%?,%?)",
             self.trait_id.repr(tcx),
             self.method_num,
             self.param_num,
             self.bound_num)
    }
}

impl Repr for typeck::method_object {
    fn repr(&self, tcx: ctxt) -> ~str {
        fmt!("method_object(%s,%?,%?)",
             self.trait_id.repr(tcx),
             self.method_num,
             self.real_index)
    }
}


impl Repr for ty::RegionVid {
    fn repr(&self, _tcx: ctxt) -> ~str {
        fmt!("%?", *self)
    }
}

impl Repr for ty::TraitStore {
    fn repr(&self, tcx: ctxt) -> ~str {
        match self {
            &ty::BoxTraitStore => ~"@Trait",
            &ty::UniqTraitStore => ~"~Trait",
            &ty::RegionTraitStore(r) => fmt!("&%s Trait", r.repr(tcx))
        }
    }
}

impl Repr for ty::vstore {
    fn repr(&self, tcx: ctxt) -> ~str {
        vstore_to_str(tcx, *self)
    }
}

impl Repr for ast_map::path_elt {
    fn repr(&self, tcx: ctxt) -> ~str {
        match *self {
            ast_map::path_mod(id) => id.repr(tcx),
            ast_map::path_name(id) => id.repr(tcx),
            ast_map::path_pretty_name(id, _) => id.repr(tcx),
        }
    }
}

impl Repr for ty::BuiltinBound {
    fn repr(&self, _tcx: ctxt) -> ~str {
        fmt!("%?", *self)
    }
}

impl UserString for ty::BuiltinBound {
    fn user_string(&self, _tcx: ctxt) -> ~str {
        match *self {
            ty::BoundStatic => ~"'static",
            ty::BoundSend => ~"Send",
            ty::BoundFreeze => ~"Freeze",
            ty::BoundSized => ~"Sized",
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
        let path = ty::item_path(tcx, self.def_id);
        let base = ast_map::path_to_str(path, tcx.sess.intr());
        if tcx.sess.verbose() && self.substs.self_ty.is_some() {
            let mut all_tps = self.substs.tps.clone();
            for &t in self.substs.self_ty.iter() { all_tps.push(t); }
            parameterized(tcx, base, &self.substs.regions, all_tps)
        } else {
            parameterized(tcx, base, &self.substs.regions, self.substs.tps)
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
