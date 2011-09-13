
import syntax::{ast, ast_util};
import ast::{ident, fn_ident, node_id, def_id};
import mut::{expr_root, mut_field, deref, field, index, unbox};
import syntax::codemap::span;
import syntax::visit;
import visit::vt;
import std::{vec, str, option};
import std::option::{some, none, is_none};

// This is not an alias-analyser (though it would merit from becoming one, or
// getting input from one, to be more precise). It is a pass that checks
// whether aliases are used in a safe way.

tag valid { valid; overwritten(span, ast::path); val_taken(span, ast::path); }
tag copied { not_allowed; copied; not_copied; }

type restrict = @{node_id: node_id,
                  span: span,
                  local_id: uint,
                  root_var: option::t<node_id>,
                  unsafe_tys: [ty::t],
                  mutable ok: valid,
                  mutable copied: copied};

type scope = @[restrict];

tag local_info { local(uint); }

type copy_map = std::map::hashmap<node_id, ()>;

type ctx =
    {tcx: ty::ctxt,
     local_map: std::map::hashmap<node_id, local_info>,
     mutable next_local: uint,
     copy_map: copy_map};

fn check_crate(tcx: ty::ctxt, crate: @ast::crate) -> copy_map {
    // Stores information about object fields and function
    // arguments that's otherwise not easily available.
    let cx =
        @{tcx: tcx,
          local_map: std::map::new_int_hash(),
          mutable next_local: 0u,
          copy_map: std::map::new_int_hash()};
    let v =
        @{visit_fn: visit_fn,
          visit_expr: bind visit_expr(cx, _, _, _),
          visit_decl: bind visit_decl(cx, _, _, _)
             with *visit::default_visitor::<scope>()};
    visit::visit_crate(*crate, @[], visit::mk_vt(v));
    tcx.sess.abort_if_errors();
    ret cx.copy_map;
}

fn visit_fn(f: ast::_fn, _tp: [ast::ty_param], _sp: span, _name: fn_ident,
            _id: ast::node_id, sc: scope, v: vt<scope>) {
    visit::visit_fn_decl(f.decl, sc, v);
    let scope = alt f.proto {
      // Blocks need to obey any restrictions from the enclosing scope.
      ast::proto_block. | ast::proto_closure. { sc }
      // Non capturing functions start out fresh.
      _ { @[] }
    };
    v.visit_block(f.body, scope, v);
}

fn visit_expr(cx: @ctx, ex: @ast::expr, sc: scope, v: vt<scope>) {
    let handled = true;
    alt ex.node {
      ast::expr_call(f, args) {
        check_call(*cx, f, args);
        handled = false;
      }
      ast::expr_alt(input, arms) { check_alt(*cx, input, arms, sc, v); }
      ast::expr_put(val) {
        alt val {
          some(ex) {
            let root = expr_root(cx.tcx, ex, false);
            if mut_field(root.ds) {
                cx.tcx.sess.span_err(ex.span,
                                     "result of put must be" +
                                         " immutably rooted");
            }
            visit_expr(cx, ex, sc, v);
          }
          _ { }
        }
      }
      ast::expr_for_each(decl, call, blk) {
        check_for_each(*cx, decl, call, blk, sc, v);
      }
      ast::expr_for(decl, seq, blk) { check_for(*cx, decl, seq, blk, sc, v); }
      ast::expr_path(pt) {
        check_var(*cx, ex, pt, ex.id, false, sc);
        handled = false;
      }
      ast::expr_swap(lhs, rhs) {
        check_lval(cx, lhs, sc, v);
        check_lval(cx, rhs, sc, v);
        handled = false;
      }
      ast::expr_move(dest, src) {
        check_assign(cx, dest, src, sc, v);
        check_lval(cx, src, sc, v);
      }
      ast::expr_assign(dest, src) | ast::expr_assign_op(_, dest, src) {
        check_assign(cx, dest, src, sc, v);
      }
      _ { handled = false; }
    }
    if !handled { visit::visit_expr(ex, sc, v); }
}

fn register_locals(cx: ctx, pat: @ast::pat) {
    for each pat in ast_util::pat_bindings(pat) {
        cx.local_map.insert(pat.id, local(cx.next_local));
        cx.next_local += 1u;
    }
}

fn visit_decl(cx: @ctx, d: @ast::decl, sc: scope, v: vt<scope>) {
    visit::visit_decl(d, sc, v);
    alt d.node {
      ast::decl_local(locs) {
        for loc: @ast::local in locs {
            alt loc.node.init {
              some(init) {
                if init.op == ast::init_move {
                    check_lval(cx, init.expr, sc, v);
                }
              }
              none. { }
            }
            register_locals(*cx, loc.node.pat);
        }
      }
      _ { }
    }
}

fn cant_copy(cx: ctx, r: restrict) -> bool {
    alt r.copied {
      not_allowed. { ret true; }
      copied. { ret false; }
      not_copied. {}
    }
    let ty = ty::node_id_to_type(cx.tcx, r.node_id);
    if ty::type_allows_implicit_copy(cx.tcx, ty) {
        r.copied = copied;
        cx.copy_map.insert(r.node_id, ());
        if copy_is_expensive(cx.tcx, ty) {
            cx.tcx.sess.span_warn(r.span,
                                  "inserting an implicit copy for type " +
                                  util::ppaux::ty_to_str(cx.tcx, ty));
        }
        ret false;
    } else { ret true; }
}

fn check_call(cx: ctx, f: @ast::expr, args: [@ast::expr]) -> [restrict] {
    let fty = ty::type_autoderef(cx.tcx, ty::expr_ty(cx.tcx, f));
    let arg_ts = ty::ty_fn_args(cx.tcx, fty);
    let mut_roots: [{arg: uint, node: node_id}] = [];
    let restricts = [];
    let i = 0u;
    for arg_t: ty::arg in arg_ts {
        let arg = args[i];
        let root = expr_root(cx.tcx, arg, false);
        if arg_t.mode == ast::by_mut_ref {
            alt path_def(cx, arg) {
              some(def) {
                let dnum = ast_util::def_id_of_def(def).node;
                mut_roots += [{arg: i, node: dnum}];
              }
              _ { }
            }
        }
        let root_var = path_def_id(cx, root.ex);
        restricts += [@{node_id: arg.id,
                        span: arg.span,
                        local_id: cx.next_local,
                        root_var: root_var,
                        unsafe_tys: inner_mut(root.ds),
                        mutable ok: valid,
                        mutable copied: alt arg_t.mode {
                          ast::by_move. { copied }
                          ast::by_ref. { not_copied }
                          ast::by_mut_ref. { not_allowed }
                        }}];
        i += 1u;
    }
    let f_may_close =
        alt f.node {
          ast::expr_path(_) { def_is_local(cx.tcx.def_map.get(f.id), true) }
          _ { true }
        };
    if f_may_close {
        let i = 0u;
        for r in restricts {
            if vec::len(r.unsafe_tys) > 0u && cant_copy(cx, r) {
                cx.tcx.sess.span_err(f.span,
                                     #fmt["function may alias with argument \
                                           %u, which is not immutably rooted",
                                          i]);
            }
            i += 1u;
        }
    }
    let j = 0u;
    for r in restricts {
        for ty in r.unsafe_tys {
            let i = 0u;
            for arg_t: ty::arg in arg_ts {
                let mut_alias = arg_t.mode == ast::by_mut_ref;
                if i != j &&
                       ty_can_unsafely_include(cx, ty, arg_t.ty, mut_alias) &&
                       cant_copy(cx, r) {
                    cx.tcx.sess.span_err
                        (args[i].span,
                         #fmt["argument %u may alias with argument %u, \
                              which is not immutably rooted",
                                              i, j]);
                }
                i += 1u;
            }
        }
        j += 1u;
    }
    // Ensure we're not passing a root by mutable alias.

    for {node: node, arg: arg} in mut_roots {
        let i = 0u;
        for r in restricts {
            if i != arg {
                alt r.root_var {
                  some(root) {
                    if node == root && cant_copy(cx, r) {
                        cx.tcx.sess.span_err
                            (args[arg].span,
                             "passing a mutable reference to a \
                              variable that roots another reference");
                        break;
                    }
                  }
                  none. { }
                }
            }
            i += 1u;
        }
    }
    ret restricts;
}

fn check_alt(cx: ctx, input: @ast::expr, arms: [ast::arm], sc: scope,
             v: vt<scope>) {
    v.visit_expr(input, sc, v);
    let root = expr_root(cx.tcx, input, true);
    for a: ast::arm in arms {
        let new_sc = *sc;
        let root_var = path_def_id(cx, root.ex);
        let pat_id_map = ast_util::pat_id_map(a.pats[0]);
        type info = {id: node_id, mutable unsafe: [ty::t], span: span};
        let binding_info: [info] = [];
        for pat in a.pats {
            for proot in *pattern_roots(cx.tcx, *root.ds, pat) {
                let canon_id = pat_id_map.get(proot.name);
                // FIXME I wanted to use a block, but that hit a
                // typestate bug.
                fn match(x: info, canon: node_id) -> bool { x.id == canon }
                alt vec::find(bind match(_, canon_id), binding_info) {
                  some(s) { s.unsafe += inner_mut(proot.ds); }
                  none. {
                      binding_info += [{id: canon_id,
                                        mutable unsafe: inner_mut(proot.ds),
                                        span: proot.span}];
                  }
                }
            }
        }
        for info in binding_info {
            new_sc += [@{node_id: info.id,
                         span: info.span,
                         local_id: cx.next_local,
                         root_var: root_var,
                         unsafe_tys: info.unsafe,
                         mutable ok: valid,
                         mutable copied: not_copied}];
        }
        register_locals(cx, a.pats[0]);
        visit::visit_arm(a, @new_sc, v);
    }
}

fn check_for_each(cx: ctx, local: @ast::local, call: @ast::expr,
                  blk: ast::blk, sc: scope, v: vt<scope>) {
    v.visit_expr(call, sc, v);
    alt call.node {
      ast::expr_call(f, args) {
        let new_sc = *sc + check_call(cx, f, args);
        for proot in *pattern_roots(cx.tcx, [], local.node.pat) {
            new_sc += [@{node_id: proot.id,
                         span: proot.span,
                         local_id: cx.next_local,
                         root_var: none::<node_id>,
                         unsafe_tys: inner_mut(proot.ds),
                         mutable ok: valid,
                         mutable copied: not_copied}];
        }
        register_locals(cx, local.node.pat);
        visit::visit_block(blk, @new_sc, v);
      }
    }
}

fn check_for(cx: ctx, local: @ast::local, seq: @ast::expr, blk: ast::blk,
             sc: scope, v: vt<scope>) {
    v.visit_expr(seq, sc, v);
    let root = expr_root(cx.tcx, seq, false);

    // If this is a mutable vector, don't allow it to be touched.
    let seq_t = ty::expr_ty(cx.tcx, seq);
    let ext_ds = *root.ds;
    alt ty::struct(cx.tcx, seq_t) {
      ty::ty_vec(mt) {
        if mt.mut != ast::imm {
            ext_ds += [@{mut: true, kind: index, outer_t: seq_t}];
        }
      }
      _ {}
    }
    let root_var = path_def_id(cx, root.ex);
    let new_sc = *sc;
    for proot in *pattern_roots(cx.tcx, ext_ds, local.node.pat) {
        new_sc += [@{node_id: proot.id,
                     span: proot.span,
                     local_id: cx.next_local,
                     root_var: root_var,
                     unsafe_tys: inner_mut(proot.ds),
                     mutable ok: valid,
                     mutable copied: not_copied}];
    }
    register_locals(cx, local.node.pat);
    visit::visit_block(blk, @new_sc, v);
}

fn check_var(cx: ctx, ex: @ast::expr, p: ast::path, id: ast::node_id,
             assign: bool, sc: scope) {
    let def = cx.tcx.def_map.get(id);
    if !def_is_local(def, true) { ret; }
    let my_defnum = ast_util::def_id_of_def(def).node;
    let my_local_id =
        alt cx.local_map.find(my_defnum) { some(local(id)) { id } _ { 0u } };
    let var_t = ty::expr_ty(cx.tcx, ex);
    for r: restrict in *sc {
        // excludes variables introduced since the alias was made
        if my_local_id < r.local_id {
            for ty in r.unsafe_tys {
                if ty_can_unsafely_include(cx, ty, var_t, assign) {
                    r.ok = val_taken(ex.span, p);
                }
            }
        } else if r.node_id == my_defnum {
            test_scope(cx, sc, r, p);
        }
    }
}

fn check_lval(cx: @ctx, dest: @ast::expr, sc: scope, v: vt<scope>) {
    alt dest.node {
      ast::expr_path(p) {
        let def = cx.tcx.def_map.get(dest.id);
        let dnum = ast_util::def_id_of_def(def).node;
        for r: restrict in *sc {
            if r.root_var == some(dnum) { r.ok = overwritten(dest.span, p); }
        }
      }
      _ { visit_expr(cx, dest, sc, v); }
    }
}

fn check_assign(cx: @ctx, dest: @ast::expr, src: @ast::expr, sc: scope,
                v: vt<scope>) {
    visit_expr(cx, src, sc, v);
    check_lval(cx, dest, sc, v);
}

fn test_scope(cx: ctx, sc: scope, r: restrict, p: ast::path) {
    let prob = r.ok;
    alt r.root_var {
      some(dn) {
        for other in *sc {
            if other.node_id == dn {
                prob = other.ok;
                if prob != valid { break; }
            }
        }
      }
      _ {}
    }
    if prob != valid && cant_copy(cx, r) {
        let msg = alt prob {
          overwritten(sp, wpt) {
            {span: sp, msg: "overwriting " + ast_util::path_name(wpt)}
          }
          val_taken(sp, vpt) {
            {span: sp,
             msg: "taking the value of " + ast_util::path_name(vpt)}
          }
        };
        cx.tcx.sess.span_err(msg.span,
                             msg.msg + " will invalidate reference " +
                                 ast_util::path_name(p) +
                                 ", which is still used");
    }
}

fn path_def(cx: ctx, ex: @ast::expr) -> option::t<ast::def> {
    ret alt ex.node {
          ast::expr_path(_) { some(cx.tcx.def_map.get(ex.id)) }
          _ { none }
        }
}

fn path_def_id(cx: ctx, ex: @ast::expr) -> option::t<ast::node_id> {
    alt ex.node {
      ast::expr_path(_) {
        ret some(ast_util::def_id_of_def(cx.tcx.def_map.get(ex.id)).node);
      }
      _ { ret none; }
    }
}

fn ty_can_unsafely_include(cx: ctx, needle: ty::t, haystack: ty::t, mut: bool)
   -> bool {
    fn get_mut(cur: bool, mt: ty::mt) -> bool {
        ret cur || mt.mut != ast::imm;
    }
    fn helper(tcx: ty::ctxt, needle: ty::t, haystack: ty::t, mut: bool) ->
       bool {
        if needle == haystack { ret true; }
        alt ty::struct(tcx, haystack) {
          ty::ty_tag(_, ts) {
            for t: ty::t in ts {
                if helper(tcx, needle, t, mut) { ret true; }
            }
            ret false;
          }
          ty::ty_box(mt) | ty::ty_ptr(mt) {
            ret helper(tcx, needle, mt.ty, get_mut(mut, mt));
          }
          ty::ty_uniq(t) { ret helper(tcx, needle, t, false); }
          ty::ty_rec(fields) {
            for f: ty::field in fields {
                if helper(tcx, needle, f.mt.ty, get_mut(mut, f.mt)) {
                    ret true;
                }
            }
            ret false;
          }
          ty::ty_tup(ts) {
            for t in ts { if helper(tcx, needle, t, mut) { ret true; } }
            ret false;
          }





          // These may contain anything.
          ty::ty_fn(_, _, _, _, _) {
            ret true;
          }
          ty::ty_obj(_) { ret true; }





          // A type param may include everything, but can only be
          // treated as opaque downstream, and is thus safe unless we
          // saw mutable fields, in which case the whole thing can be
          // overwritten.
          ty::ty_param(_, _) {
            ret mut;
          }
          _ { ret false; }
        }
    }
    ret helper(cx.tcx, needle, haystack, mut);
}

fn def_is_local(d: ast::def, objfields_count: bool) -> bool {
    ret alt d {
          ast::def_local(_) | ast::def_arg(_, _) | ast::def_binding(_) |
          ast::def_upvar(_, _, _) {
            true
          }
          ast::def_obj_field(_, _) { objfields_count }
          _ { false }
        };
}

// Heuristic, somewhat random way to decide whether to warn when inserting an
// implicit copy.
fn copy_is_expensive(tcx: ty::ctxt, ty: ty::t) -> bool {
    fn score_ty(tcx: ty::ctxt, ty: ty::t) -> uint {
        ret alt ty::struct(tcx, ty) {
          ty::ty_nil. | ty::ty_bot. | ty::ty_bool. | ty::ty_int. |
          ty::ty_uint. | ty::ty_float. | ty::ty_machine(_) |
          ty::ty_char. | ty::ty_type. | ty::ty_native(_) |
          ty::ty_ptr(_) { 1u }
          ty::ty_box(_) { 3u }
          ty::ty_constr(t, _) | ty::ty_res(_, t, _) { score_ty(tcx, t) }
          ty::ty_fn(_, _, _, _, _) | ty::ty_native_fn(_, _, _) |
          ty::ty_obj(_) { 4u }
          ty::ty_str. | ty::ty_vec(_) | ty::ty_param(_, _) { 50u }
          ty::ty_uniq(t) { 1u + score_ty(tcx, t) }
          ty::ty_tag(_, ts) | ty::ty_tup(ts) {
            let sum = 0u;
            for t in ts { sum += score_ty(tcx, t); }
            sum
          }
          ty::ty_rec(fs) {
            let sum = 0u;
            for f in fs { sum += score_ty(tcx, f.mt.ty); }
            sum
          }
        };
    }
    ret score_ty(tcx, ty) > 8u;
}

type pattern_root = {id: node_id, name: ident, ds: @[deref], span: span};

fn pattern_roots(tcx: ty::ctxt, base: [deref], pat: @ast::pat)
    -> @[pattern_root] {
    fn walk(tcx: ty::ctxt, base: [deref], pat: @ast::pat,
            &set: [pattern_root]) {
        alt pat.node {
          ast::pat_wild. | ast::pat_lit(_) {}
          ast::pat_bind(nm) {
            set += [{id: pat.id, name: nm, ds: @base, span: pat.span}];
          }
          ast::pat_tag(_, ps) | ast::pat_tup(ps) {
            let base = base + [@{mut: false, kind: field,
                                 outer_t: ty::node_id_to_type(tcx, pat.id)}];
            for p in ps { walk(tcx, base, p, set); }
          }
          ast::pat_rec(fs, _) {
            let ty = ty::node_id_to_type(tcx, pat.id);
            for f in fs {
                let mut = ty::get_field(tcx, ty, f.ident).mt.mut != ast::imm;
                let base = base + [@{mut: mut, kind: field, outer_t: ty}];
                walk(tcx, base, f.pat, set);
            }
          }
          ast::pat_box(p) {
            let ty = ty::node_id_to_type(tcx, pat.id);
            let mut = alt ty::struct(tcx, ty) {
              ty::ty_box(mt) { mt.mut != ast::imm }
            };
            walk(tcx, base + [@{mut: mut, kind: unbox, outer_t: ty}], p, set);
          }
        }
    }
    let set = [];
    walk(tcx, base, pat, set);
    ret @set;
}

fn inner_mut(ds: @[deref]) -> [ty::t] {
    for d: deref in *ds { if d.mut { ret [d.outer_t]; } }
    ret [];
}

// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C $RBUILD 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
