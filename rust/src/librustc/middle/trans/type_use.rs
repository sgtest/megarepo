// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Determines the ways in which a generic function body depends
// on its type parameters. Used to aggressively reuse compiled
// function bodies for different types.

// This unfortunately depends on quite a bit of knowledge about the
// details of the language semantics, and is likely to accidentally go
// out of sync when something is changed. It could be made more
// powerful by distinguishing between functions that only need to know
// the size and alignment of a type, and those that also use its
// drop/take glue. But this would increase the fragility of the code
// to a ridiculous level, and probably not catch all that many extra
// opportunities for reuse.

// (An other approach to doing what this code does is to instrument
// the translation code to set flags whenever it does something like
// alloca a type or get a tydesc. This would not duplicate quite as
// much information, but have the disadvantage of being very
// invasive.)

use middle::freevars;
use middle::trans::common::*;
use middle::trans::inline;
use middle::ty;
use middle::typeck;

use std::option::{Some, None};
use std::vec;
use extra::list::{List, Cons, Nil};
use extra::list;
use syntax::ast;
use syntax::ast::*;
use syntax::ast_map;
use syntax::ast_util;
use syntax::parse::token;
use syntax::visit;
use syntax::visit::Visitor;

pub type type_uses = uint; // Bitmask
pub static use_repr: uint = 1;   /* Dependency on size/alignment/mode and
                                     take/drop glue */
pub static use_tydesc: uint = 2; /* Takes the tydesc, or compares */
pub static use_all: uint = use_repr|use_tydesc;

#[deriving(Clone)]
pub struct Context {
    ccx: @mut CrateContext,
    uses: @mut ~[type_uses]
}

pub fn type_uses_for(ccx: @mut CrateContext, fn_id: DefId, n_tps: uint)
    -> @~[type_uses] {

    fn store_type_uses(cx: Context, fn_id: DefId) -> @~[type_uses] {
        let Context { uses, ccx } = cx;
        let uses = @(*uses).clone(); // freeze
        ccx.type_use_cache.insert(fn_id, uses);
        uses
    }

    match ccx.type_use_cache.find(&fn_id) {
      Some(uses) => return *uses,
      None => ()
    }

    let fn_id_loc = if fn_id.crate == LOCAL_CRATE {
        fn_id
    } else {
        inline::maybe_instantiate_inline(ccx, fn_id)
    };

    // Conservatively assume full use for recursive loops
    ccx.type_use_cache.insert(fn_id, @vec::from_elem(n_tps, use_all));

    let cx = Context {
        ccx: ccx,
        uses: @mut vec::from_elem(n_tps, 0u)
    };

    // If the method is a default method, we mark all of the types as
    // used.  This is imprecise, but simple. Getting it right is
    // tricky because the substs on the call and the substs on the
    // default method differ, because of substs on the trait/impl.
    let is_default = ty::provided_source(ccx.tcx, fn_id_loc).is_some();
    // We also mark all of the params as used if it is an extern thing
    // that we haven't been able to inline yet.
    if is_default || fn_id_loc.crate != LOCAL_CRATE {
        for n in range(0u, n_tps) { cx.uses[n] |= use_all; }
        return store_type_uses(cx, fn_id);
    }

    let map_node = match ccx.tcx.items.find(&fn_id_loc.node) {
        Some(x) => {
            (*x).clone()
        }
        None => {
            ccx.sess.bug(fmt!("type_uses_for: unbound item ID %?",
                              fn_id_loc))
        }
    };

    match map_node {
      ast_map::node_item(@ast::item { node: item_fn(_, _, _, _, ref body),
                                      _ }, _) |
      ast_map::node_method(@ast::method {body: ref body, _}, _, _) => {
        handle_body(&cx, body);
      }
      ast_map::node_trait_method(*) => {
        // This will be a static trait method. For now, we just assume
        // it fully depends on all of the type information. (Doing
        // otherwise would require finding the actual implementation).
        for n in range(0u, n_tps) { cx.uses[n] |= use_all;}
        // We need to return early, before the arguments are processed,
        // because of difficulties in the handling of Self.
        return store_type_uses(cx, fn_id);
      }
      ast_map::node_variant(_, _, _) => {
        for n in range(0u, n_tps) { cx.uses[n] |= use_repr;}
      }
      ast_map::node_foreign_item(i@@foreign_item {
            node: foreign_item_fn(*),
            _
        },
        abi,
        _,
        _) => {
        if abi.is_intrinsic() {
            let nm = cx.ccx.sess.str_of(i.ident);
            let name = nm.as_slice();
            let flags = if name.starts_with("atomic_") {
                0
            } else {
                match name {
                    "size_of"  | "pref_align_of" | "min_align_of" |
                    "uninit"   | "init" | "transmute" | "move_val" |
                    "move_val_init" => use_repr,

                    "get_tydesc" | "needs_drop" | "contains_managed" => use_tydesc,

                    "visit_tydesc"  | "forget" | "frame_address" |
                    "morestack_addr" => 0,

                    "offset" |
                    "memcpy32" | "memcpy64" | "memmove32" | "memmove64" |
                    "memset32" | "memset64" => use_repr,

                    "sqrtf32" | "sqrtf64" | "powif32" | "powif64" |
                    "sinf32"  | "sinf64"  | "cosf32"  | "cosf64"  |
                    "powf32"  | "powf64"  | "expf32"  | "expf64"  |
                    "exp2f32" | "exp2f64" | "logf32"  | "logf64"  |
                    "log10f32"| "log10f64"| "log2f32" | "log2f64" |
                    "fmaf32"  | "fmaf64"  | "fabsf32" | "fabsf64" |
                    "floorf32"| "floorf64"| "ceilf32" | "ceilf64" |
                    "truncf32"| "truncf64" => 0,

                    "ctpop8" | "ctpop16" | "ctpop32" | "ctpop64" => 0,

                    "ctlz8" | "ctlz16" | "ctlz32" | "ctlz64" => 0,
                    "cttz8" | "cttz16" | "cttz32" | "cttz64" => 0,

                    "bswap16" | "bswap32" | "bswap64" => 0,


                    "i8_add_with_overflow"  | "u8_add_with_overflow" |
                    "i16_add_with_overflow" | "u16_add_with_overflow" |
                    "i32_add_with_overflow" | "u32_add_with_overflow" |
                    "i64_add_with_overflow" | "u64_add_with_overflow" => 0,

                    "i8_sub_with_overflow"  | "u8_sub_with_overflow" |
                    "i16_sub_with_overflow" | "u16_sub_with_overflow" |
                    "i32_sub_with_overflow" | "u32_sub_with_overflow" |
                    "i64_sub_with_overflow" | "u64_sub_with_overflow" => 0,

                    "i8_mul_with_overflow"  | "u8_mul_with_overflow" |
                    "i16_mul_with_overflow" | "u16_mul_with_overflow" |
                    "i32_mul_with_overflow" | "u32_mul_with_overflow" |
                    "i64_mul_with_overflow" | "u64_mul_with_overflow" => 0,

                    // would be cool to make these an enum instead of
                    // strings!
                    _ => fail!("unknown intrinsic in type_use")
                }
            };
            for n in range(0u, n_tps) { cx.uses[n] |= flags;}
        }
      }
      ast_map::node_struct_ctor(*) => {
        // Similarly to node_variant, this monomorphized function just
        // uses the representations of all of its type parameters.
        for n in range(0u, n_tps) { cx.uses[n] |= use_repr; }
      }
      _ => {
        ccx.tcx.sess.bug(fmt!("unknown node type in type_use: %s",
                              ast_map::node_id_to_str(
                                ccx.tcx.items,
                                fn_id_loc.node,
                                token::get_ident_interner())));
      }
    }

    // Now handle arguments
    match ty::get(ty::lookup_item_type(cx.ccx.tcx, fn_id).ty).sty {
        ty::ty_bare_fn(ty::BareFnTy {sig: ref sig, _}) |
        ty::ty_closure(ty::ClosureTy {sig: ref sig, _}) => {
            for arg in sig.inputs.iter() {
                type_needs(&cx, use_repr, *arg);
            }
        }
        _ => ()
    }

    store_type_uses(cx, fn_id)
}

pub fn type_needs(cx: &Context, use_: uint, ty: ty::t) {
    // Optimization -- don't descend type if all params already have this use
    if cx.uses.iter().any(|&elt| elt & use_ != use_) {
        type_needs_inner(cx, use_, ty, @Nil);
    }
}

pub fn type_needs_inner(cx: &Context,
                        use_: uint,
                        ty: ty::t,
                        enums_seen: @List<DefId>) {
    do ty::maybe_walk_ty(ty) |ty| {
        if ty::type_has_params(ty) {
            match ty::get(ty).sty {
                /*
                 This previously included ty_box -- that was wrong
                 because if we cast an @T to an trait (for example) and return
                 it, we depend on the drop glue for T (we have to write the
                 right tydesc into the result)
                 */
                ty::ty_closure(*) |
                ty::ty_bare_fn(*) |
                ty::ty_ptr(_) |
                ty::ty_rptr(_, _) |
                ty::ty_trait(*) => false,

              ty::ty_enum(did, ref substs) => {
                if list::find(enums_seen, |id| *id == did).is_none() {
                    let seen = @Cons(did, enums_seen);
                    let r = ty::enum_variants(cx.ccx.tcx, did);
                    for v in r.iter() {
                        for aty in v.args.iter() {
                            let t = ty::subst(cx.ccx.tcx, &(*substs), *aty);
                            type_needs_inner(cx, use_, t, seen);
                        }
                    }
                }
                false
              }
              ty::ty_param(p) => {
                cx.uses[p.idx] |= use_;
                false
              }
              _ => true
            }
        } else { false }
    }
}

pub fn node_type_needs(cx: &Context, use_: uint, id: NodeId) {
    type_needs(cx, use_, ty::node_id_to_type(cx.ccx.tcx, id));
}

pub fn mark_for_method_call(cx: &Context, e_id: NodeId, callee_id: NodeId) {
    let mut opt_static_did = None;
    {
        let r = cx.ccx.maps.method_map.find(&e_id);
        for mth in r.iter() {
            match mth.origin {
              typeck::method_static(did) => {
                  opt_static_did = Some(did);
              }
              typeck::method_param(typeck::method_param {
                  param_num: typeck::param_numbered(param),
                  _
              }) => {
                cx.uses[param] |= use_tydesc;
              }
              _ => (),
            }
        }
    }

    // Note: we do not execute this code from within the each() call
    // above because the recursive call to `type_needs` can trigger
    // inlining and hence can cause `method_map` and
    // `node_type_substs` to be modified.
    for &did in opt_static_did.iter() {
        {
            let r = cx.ccx.tcx.node_type_substs.find_copy(&callee_id);
            for ts in r.iter() {
                let type_uses = type_uses_for(cx.ccx, did, ts.len());
                for (uses, subst) in type_uses.iter().zip(ts.iter()) {
                    type_needs(cx, *uses, *subst)
                }
            }
        }
    }
}

pub fn mark_for_expr(cx: &Context, e: &Expr) {
    match e.node {
      ExprVstore(_, _) | ExprVec(_, _) | ExprStruct(*) | ExprTup(_) |
      ExprUnary(_, UnBox(_), _) | ExprUnary(_, UnUniq, _) |
      ExprBinary(_, BiAdd, _, _) | ExprRepeat(*) => {
        node_type_needs(cx, use_repr, e.id);
      }
      ExprCast(base, _) => {
        let result_t = ty::node_id_to_type(cx.ccx.tcx, e.id);
        match ty::get(result_t).sty {
            ty::ty_trait(*) => {
              // When we're casting to an trait, we need the
              // tydesc for the expr that's being cast.
              node_type_needs(cx, use_tydesc, base.id);
            }
            _ => ()
        }
      }
      ExprBinary(_, op, lhs, _) => {
        match op {
          BiEq | BiLt | BiLe | BiNe | BiGe | BiGt => {
            node_type_needs(cx, use_tydesc, lhs.id)
          }
          _ => ()
        }
      }
      ExprPath(_) | ExprSelf => {
        let opt_ts = cx.ccx.tcx.node_type_substs.find_copy(&e.id);
        for ts in opt_ts.iter() {
            let id = ast_util::def_id_of_def(cx.ccx.tcx.def_map.get_copy(&e.id));
            let uses_for_ts = type_uses_for(cx.ccx, id, ts.len());
            for (uses, subst) in uses_for_ts.iter().zip(ts.iter()) {
                type_needs(cx, *uses, *subst)
            }
        }
      }
      ExprFnBlock(*) => {
          match ty::ty_closure_sigil(ty::expr_ty(cx.ccx.tcx, e)) {
              ast::OwnedSigil => {}
              ast::BorrowedSigil | ast::ManagedSigil => {
                  for fv in freevars::get_freevars(cx.ccx.tcx, e.id).iter() {
                      let node_id = ast_util::def_id_of_def(fv.def).node;
                      node_type_needs(cx, use_repr, node_id);
                  }
              }
          }
      }
      ExprAssign(val, _) | ExprAssignOp(_, _, val, _) |
      ExprRet(Some(val)) => {
        node_type_needs(cx, use_repr, val.id);
      }
      ExprIndex(callee_id, base, _) => {
        // FIXME (#2537): could be more careful and not count fields after
        // the chosen field.
        let base_ty = ty::node_id_to_type(cx.ccx.tcx, base.id);
        type_needs(cx, use_repr, ty::type_autoderef(cx.ccx.tcx, base_ty));
        mark_for_method_call(cx, e.id, callee_id);
      }
      ExprField(base, _, _) => {
        // Method calls are now a special syntactic form,
        // so `a.b` should always be a field.
        assert!(!cx.ccx.maps.method_map.contains_key(&e.id));

        let base_ty = ty::node_id_to_type(cx.ccx.tcx, base.id);
        type_needs(cx, use_repr, ty::type_autoderef(cx.ccx.tcx, base_ty));
      }
      ExprCall(f, _, _) => {
          let r = ty::ty_fn_args(ty::node_id_to_type(cx.ccx.tcx, f.id));
          for a in r.iter() {
              type_needs(cx, use_repr, *a);
          }
      }
      ExprMethodCall(callee_id, rcvr, _, _, _, _) => {
        let base_ty = ty::node_id_to_type(cx.ccx.tcx, rcvr.id);
        type_needs(cx, use_repr, ty::type_autoderef(cx.ccx.tcx, base_ty));

        let r = ty::ty_fn_args(ty::node_id_to_type(cx.ccx.tcx, callee_id));
        for a in r.iter() {
            type_needs(cx, use_repr, *a);
        }
        mark_for_method_call(cx, e.id, callee_id);
      }

      ExprInlineAsm(ref ia) => {
        for &(_, input) in ia.inputs.iter() {
          node_type_needs(cx, use_repr, input.id);
        }
        for &(_, out) in ia.outputs.iter() {
          node_type_needs(cx, use_repr, out.id);
        }
      }

      ExprParen(e) => mark_for_expr(cx, e),

      ExprMatch(*) | ExprBlock(_) | ExprIf(*) | ExprWhile(*) |
      ExprBreak(_) | ExprAgain(_) | ExprUnary(*) | ExprLit(_) |
      ExprMac(_) | ExprAddrOf(*) | ExprRet(_) | ExprLoop(*) |
      ExprDoBody(_) | ExprLogLevel => (),

      ExprForLoop(*) => fail!("non-desugared expr_for_loop")
    }
}

struct TypeUseVisitor;

impl<'self> Visitor<&'self Context> for TypeUseVisitor {

    fn visit_expr<'a>(&mut self, e:@Expr, cx: &'a Context) {
            visit::walk_expr(self, e, cx);
            mark_for_expr(cx, e);
    }

    fn visit_local<'a>(&mut self, l:@Local, cx: &'a Context) {
            visit::walk_local(self, l, cx);
            node_type_needs(cx, use_repr, l.id);
    }

    fn visit_pat<'a>(&mut self, p:@Pat, cx: &'a Context) {
            visit::walk_pat(self, p, cx);
            node_type_needs(cx, use_repr, p.id);
    }

    fn visit_block<'a>(&mut self, b:&Block, cx: &'a Context) {
            visit::walk_block(self, b, cx);
            for e in b.expr.iter() {
                node_type_needs(cx, use_repr, e.id);
            }
    }

    fn visit_item<'a>(&mut self, _:@item, _: &'a Context) {
        // do nothing
    }

}

pub fn handle_body(cx: &Context, body: &Block) {
    let mut v = TypeUseVisitor;
    v.visit_block(body, cx);
}
