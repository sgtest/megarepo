import syntax::ast::*;
import syntax::{visit, ast_util};
import driver::session::session;
import std::map::hashmap;

fn check_crate(sess: session, crate: @crate, def_map: resolve::def_map,
                method_map: typeck::method_map, tcx: ty::ctxt) {
    visit::visit_crate(*crate, false, visit::mk_vt(@{
        visit_item: check_item,
        visit_pat: check_pat,
        visit_expr: bind check_expr(sess, def_map, method_map, tcx, _, _, _)
        with *visit::default_visitor()
    }));
    sess.abort_if_errors();
}

fn check_item(it: @item, &&_is_const: bool, v: visit::vt<bool>) {
    alt it.node {
      item_const(_, ex) { v.visit_expr(ex, true, v); }
      item_enum(vs, _) {
        for var in vs {
            option::with_option_do(var.node.disr_expr) {|ex|
                v.visit_expr(ex, true, v);
            }
        }
      }
      _ { visit::visit_item(it, false, v); }
    }
}

fn check_pat(p: @pat, &&_is_const: bool, v: visit::vt<bool>) {
    fn is_str(e: @expr) -> bool {
        alt e.node { expr_lit(@{node: lit_str(_), _}) { true } _ { false } }
    }
    alt p.node {
      // Let through plain string literals here
      pat_lit(a) { if !is_str(a) { v.visit_expr(a, true, v); } }
      pat_range(a, b) {
        if !is_str(a) { v.visit_expr(a, true, v); }
        if !is_str(b) { v.visit_expr(b, true, v); }
      }
      _ { visit::visit_pat(p, false, v); }
    }
}

fn check_expr(sess: session, def_map: resolve::def_map,
              method_map: typeck::method_map, tcx: ty::ctxt,
              e: @expr, &&is_const: bool, v: visit::vt<bool>) {
    if is_const {
        alt e.node {
          expr_unary(box(_), _) | expr_unary(uniq(_), _) |
          expr_unary(deref, _){
            sess.span_err(e.span,
                          "disallowed operator in constant expression");
            ret;
          }
          expr_lit(@{node: lit_str(_), _}) {
            sess.span_err(e.span,
                          "string constants are not supported");
          }
          expr_binary(_, _, _) | expr_unary(_, _) {
            if method_map.contains_key(e.id) {
                sess.span_err(e.span, "user-defined operators are not \
                                       allowed in constant expressions");
            }
          }
          expr_lit(_) {}
          expr_cast(_, _) {
            let ety = ty::expr_ty(tcx, e);
            if !ty::type_is_numeric(ety) {
                sess.span_err(e.span, "can not cast to `" +
                              util::ppaux::ty_to_str(tcx, ety) +
                              "` in a constant expression");
            }
          }
          expr_path(path) {
            alt def_map.find(e.id) {
              some(def_const(def_id)) {
                if !ast_util::is_local(def_id) {
                    sess.span_err(
                        e.span, "paths in constants may only refer to \
                                 crate-local constants");
                }
              }
              _ {
                sess.span_err(
                    e.span, "paths in constants may only refer to constants");
              }
            }
          }
          _ {
            sess.span_err(e.span,
                          "constant contains unimplemented expression type");
            ret;
          }
        }
    }
    alt e.node {
      expr_lit(@{node: lit_int(v, t), _}) {
        if t != ty_char {
            if (v as u64) > ast_util::int_ty_max(
                if t == ty_i { sess.targ_cfg.int_type } else { t }) {
                sess.span_err(e.span, "literal out of range for its type");
            }
        }
      }
      expr_lit(@{node: lit_uint(v, t), _}) {
        if v > ast_util::uint_ty_max(
            if t == ty_u { sess.targ_cfg.uint_type } else { t }) {
            sess.span_err(e.span, "literal out of range for its type");
        }
      }
      _ {}
    }
    visit::visit_expr(e, is_const, v);
}

// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
