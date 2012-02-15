import syntax::ast::*;
import syntax::visit;
import syntax::ast_util;
import driver::session::session;

enum deref_t { unbox(bool), field, index, }

type deref = @{mutbl: bool, kind: deref_t, outer_t: ty::t};

// Finds the root (the thing that is dereferenced) for the given expr, and a
// vec of dereferences that were used on this root. Note that, in this vec,
// the inner derefs come in front, so foo.bar[1] becomes rec(ex=foo,
// ds=[index,field])
fn expr_root(tcx: ty::ctxt, ex: @expr, autoderef: bool) ->
   {ex: @expr, ds: @[deref]} {
    fn maybe_auto_unbox(tcx: ty::ctxt, t: ty::t) -> {t: ty::t, ds: [deref]} {
        let ds = [], t = t;
        while true {
            alt ty::get(t).struct {
              ty::ty_box(mt) {
                ds += [@{mutbl: mt.mutbl == m_mutbl,
                         kind: unbox(false),
                         outer_t: t}];
                t = mt.ty;
              }
              ty::ty_uniq(mt) {
                ds += [@{mutbl: mt.mutbl == m_mutbl,
                         kind: unbox(false),
                         outer_t: t}];
                t = mt.ty;
              }
              ty::ty_res(_, inner, tps) {
                ds += [@{mutbl: false, kind: unbox(false), outer_t: t}];
                t = ty::substitute_type_params(tcx, tps, inner);
              }
              ty::ty_enum(did, tps) {
                let variants = ty::enum_variants(tcx, did);
                if vec::len(*variants) != 1u ||
                       vec::len(variants[0].args) != 1u {
                    break;
                }
                ds += [@{mutbl: false, kind: unbox(false), outer_t: t}];
                t = ty::substitute_type_params(tcx, tps, variants[0].args[0]);
              }
              _ { break; }
            }
        }
        ret {t: t, ds: ds};
    }
    let ds: [deref] = [], ex = ex;
    while true {
        alt copy ex.node {
          expr_field(base, ident, _) {
            let auto_unbox = maybe_auto_unbox(tcx, ty::expr_ty(tcx, base));
            let is_mutbl = false;
            alt ty::get(auto_unbox.t).struct {
              ty::ty_rec(fields) {
                for fld: ty::field in fields {
                    if str::eq(ident, fld.ident) {
                        is_mutbl = fld.mt.mutbl == m_mutbl;
                        break;
                    }
                }
              }
              _ {}
            }
            ds += [@{mutbl: is_mutbl, kind: field, outer_t: auto_unbox.t}];
            ds += auto_unbox.ds;
            ex = base;
          }
          expr_index(base, _) {
            let auto_unbox = maybe_auto_unbox(tcx, ty::expr_ty(tcx, base));
            alt ty::get(auto_unbox.t).struct {
              ty::ty_vec(mt) {
                ds +=
                    [@{mutbl: mt.mutbl == m_mutbl,
                       kind: index,
                       outer_t: auto_unbox.t}];
              }
              ty::ty_str {
                ds += [@{mutbl: false, kind: index, outer_t: auto_unbox.t}];
              }
              _ { break; }
            }
            ds += auto_unbox.ds;
            ex = base;
          }
          expr_unary(op, base) {
            if op == deref {
                let base_t = ty::expr_ty(tcx, base);
                let is_mutbl = false, ptr = false;
                alt ty::get(base_t).struct {
                  ty::ty_box(mt) { is_mutbl = mt.mutbl == m_mutbl; }
                  ty::ty_uniq(mt) { is_mutbl = mt.mutbl == m_mutbl; }
                  ty::ty_res(_, _, _) { }
                  ty::ty_enum(_, _) { }
                  ty::ty_ptr(mt) {
                    is_mutbl = mt.mutbl == m_mutbl;
                    ptr = true;
                  }
                  _ { tcx.sess.span_bug(base.span, "Ill-typed base \
                        expression in deref"); }
                }
                ds += [@{mutbl: is_mutbl, kind: unbox(ptr && is_mutbl),
                         outer_t: base_t}];
                ex = base;
            } else { break; }
          }
          _ { break; }
        }
    }
    if autoderef {
        let auto_unbox = maybe_auto_unbox(tcx, ty::expr_ty(tcx, ex));
        ds += auto_unbox.ds;
    }
    ret {ex: ex, ds: @ds};
}

// Actual mutbl-checking pass

type mutbl_map = std::map::hashmap<node_id, ()>;
type ctx = {tcx: ty::ctxt, mutbl_map: mutbl_map};

fn check_crate(tcx: ty::ctxt, crate: @crate) -> mutbl_map {
    let cx = @{tcx: tcx, mutbl_map: std::map::new_int_hash()};
    let v = @{visit_expr: bind visit_expr(cx, _, _, _),
              visit_decl: bind visit_decl(cx, _, _, _)
              with *visit::default_visitor()};
    visit::visit_crate(*crate, (), visit::mk_vt(v));
    ret cx.mutbl_map;
}

enum msg { msg_assign, msg_move_out, msg_mutbl_ref, }

fn mk_err(cx: @ctx, span: syntax::codemap::span, msg: msg, name: str) {
    cx.tcx.sess.span_err(span, alt msg {
      msg_assign { "assigning to " + name }
      msg_move_out { "moving out of " + name }
      msg_mutbl_ref { "passing " + name + " by mutable reference" }
    });
}

fn visit_decl(cx: @ctx, d: @decl, &&e: (), v: visit::vt<()>) {
    visit::visit_decl(d, e, v);
    alt d.node {
      decl_local(locs) {
        for loc in locs {
            alt loc.node.init {
              some(init) {
                if init.op == init_move { check_move_rhs(cx, init.expr); }
              }
              none { }
            }
        }
      }
      _ { }
    }
}

fn visit_expr(cx: @ctx, ex: @expr, &&e: (), v: visit::vt<()>) {
    alt ex.node {
      expr_call(f, args, _) { check_call(cx, f, args); }
      expr_bind(f, args) { check_bind(cx, f, args); }
      expr_swap(lhs, rhs) {
        check_lval(cx, lhs, msg_assign);
        check_lval(cx, rhs, msg_assign);
      }
      expr_move(dest, src) {
        check_lval(cx, dest, msg_assign);
        check_move_rhs(cx, src);
      }
      expr_assign(dest, src) | expr_assign_op(_, dest, src) {
        check_lval(cx, dest, msg_assign);
      }
      _ { }
    }
    visit::visit_expr(ex, e, v);
}

fn check_lval(cx: @ctx, dest: @expr, msg: msg) {
    alt dest.node {
      expr_path(p) {
        let def = cx.tcx.def_map.get(dest.id);
        alt is_immutable_def(cx, def) {
          some(name) { mk_err(cx, dest.span, msg, name); }
          _ { }
        }
        cx.mutbl_map.insert(ast_util::def_id_of_def(def).node, ());
      }
      _ {
        let root = expr_root(cx.tcx, dest, false);
        if vec::len(*root.ds) == 0u {
            if msg != msg_move_out {
                mk_err(cx, dest.span, msg, "non-lvalue");
            }
        } else if !root.ds[0].mutbl {
            let name =
                alt root.ds[0].kind {
                  mutbl::unbox(_) { "immutable box" }
                  mutbl::field { "immutable field" }
                  mutbl::index { "immutable vec content" }
                };
            mk_err(cx, dest.span, msg, name);
        }
      }
    }
}

fn check_move_rhs(cx: @ctx, src: @expr) {
    alt src.node {
      expr_path(p) {
        alt cx.tcx.def_map.get(src.id) {
          def_self(_) {
            mk_err(cx, src.span, msg_move_out, "method self");
          }
          _ { }
        }
        check_lval(cx, src, msg_move_out);
      }
      _ {
        let root = expr_root(cx.tcx, src, false);

        // Not a path and no-derefs means this is a temporary.
        if vec::len(*root.ds) != 0u &&
           root.ds[vec::len(*root.ds) - 1u].kind != unbox(true) {
            cx.tcx.sess.span_err(src.span, "moving out of a data structure");
        }
      }
    }
}

fn check_call(cx: @ctx, f: @expr, args: [@expr]) {
    let arg_ts = ty::ty_fn_args(ty::expr_ty(cx.tcx, f));
    let i = 0u;
    for arg_t: ty::arg in arg_ts {
        alt ty::resolved_mode(cx.tcx, arg_t.mode) {
          by_mutbl_ref { check_lval(cx, args[i], msg_mutbl_ref); }
          by_move { check_lval(cx, args[i], msg_move_out); }
          by_ref | by_val | by_copy { }
        }
        i += 1u;
    }
}

fn check_bind(cx: @ctx, f: @expr, args: [option<@expr>]) {
    let arg_ts = ty::ty_fn_args(ty::expr_ty(cx.tcx, f));
    let i = 0u;
    for arg in args {
        alt arg {
          some(expr) {
            let o_msg = alt ty::resolved_mode(cx.tcx, arg_ts[i].mode) {
              by_mutbl_ref { some("by mutable reference") }
              by_move { some("by move") }
              _ { none }
            };
            alt o_msg {
              some(name) {
                cx.tcx.sess.span_err(
                    expr.span, "can not bind an argument passed " + name);
              }
              none {}
            }
          }
          _ {}
        }
        i += 1u;
    }
}

fn is_immutable_def(cx: @ctx, def: def) -> option<str> {
    alt def {
      def_fn(_, _) | def_mod(_) | def_native_mod(_) | def_const(_) |
      def_use(_) {
        some("static item")
      }
      def_arg(_, m) {
        alt ty::resolved_mode(cx.tcx, m) {
          by_ref | by_val { some("argument") }
          by_mutbl_ref | by_move | by_copy { none }
        }
      }
      def_self(_) { some("self argument") }
      def_upvar(_, inner, node_id) {
        let ty = ty::node_id_to_type(cx.tcx, node_id);
        let proto = ty::ty_fn_proto(ty);
        ret alt proto {
          proto_any | proto_block { is_immutable_def(cx, *inner) }
          _ { some("upvar") }
        };
      }
      def_binding(_) { some("binding") }
      _ { none }
    }
}

// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
