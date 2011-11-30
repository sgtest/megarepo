import std::{str, option};
import codemap::span;
import ast::*;

fn respan<copy T>(sp: span, t: T) -> spanned<T> {
    ret {node: t, span: sp};
}

/* assuming that we're not in macro expansion */
fn mk_sp(lo: uint, hi: uint) -> span {
    ret {lo: lo, hi: hi, expanded_from: codemap::os_none};
}

// make this a const, once the compiler supports it
fn dummy_sp() -> span { ret mk_sp(0u, 0u); }

fn path_name(p: @path) -> str { path_name_i(p.node.idents) }

fn path_name_i(idents: [ident]) -> str { str::connect(idents, "::") }

fn local_def(id: node_id) -> def_id { ret {crate: local_crate, node: id}; }

fn variant_def_ids(d: def) -> {tg: def_id, var: def_id} {
    alt d { def_variant(tag_id, var_id) { ret {tg: tag_id, var: var_id}; } }
}

fn def_id_of_def(d: def) -> def_id {
    alt d {
      def_fn(id, _) { ret id; }
      def_obj_field(id, _) { ret id; }
      def_mod(id) { ret id; }
      def_native_mod(id) { ret id; }
      def_const(id) { ret id; }
      def_arg(id, _) { ret id; }
      def_local(id, _) { ret id; }
      def_variant(_, id) { ret id; }
      def_ty(id) { ret id; }
      def_ty_param(_, _) { fail; }
      def_binding(id) { ret id; }
      def_use(id) { ret id; }
      def_native_ty(id) { ret id; }
      def_native_fn(id, _) { ret id; }
      def_upvar(id, _, _) { ret id; }
    }
}

type pat_id_map = std::map::hashmap<str, node_id>;

// This is used because same-named variables in alternative patterns need to
// use the node_id of their namesake in the first pattern.
fn pat_id_map(pat: @pat) -> pat_id_map {
    let map = std::map::new_str_hash::<node_id>();
    pat_bindings(pat) {|bound|
        let name = alt bound.node { pat_bind(n) { n } };
        map.insert(name, bound.id);
    };
    ret map;
}

// FIXME: could return a constrained type
fn pat_bindings(pat: @pat, it: block(@pat)) {
    alt pat.node {
      pat_bind(_) { it(pat); }
      pat_tag(_, sub) { for p in sub { pat_bindings(p, it); } }
      pat_rec(fields, _) { for f in fields { pat_bindings(f.pat, it); } }
      pat_tup(elts) { for elt in elts { pat_bindings(elt, it); } }
      pat_box(sub) { pat_bindings(sub, it); }
      pat_uniq(sub) { pat_bindings(sub, it); }
      pat_wild. | pat_lit(_) | pat_range(_, _) { }
    }
}

fn pat_binding_ids(pat: @pat) -> [node_id] {
    let found = [];
    pat_bindings(pat) {|b| found += [b.id]; };
    ret found;
}

fn binop_to_str(op: binop) -> str {
    alt op {
      add. { ret "+"; }
      sub. { ret "-"; }
      mul. { ret "*"; }
      div. { ret "/"; }
      rem. { ret "%"; }
      and. { ret "&&"; }
      or. { ret "||"; }
      bitxor. { ret "^"; }
      bitand. { ret "&"; }
      bitor. { ret "|"; }
      lsl. { ret "<<"; }
      lsr. { ret ">>"; }
      asr. { ret ">>>"; }
      eq. { ret "=="; }
      lt. { ret "<"; }
      le. { ret "<="; }
      ne. { ret "!="; }
      ge. { ret ">="; }
      gt. { ret ">"; }
    }
}

pure fn lazy_binop(b: binop) -> bool {
    alt b { and. { true } or. { true } _ { false } }
}

fn unop_to_str(op: unop) -> str {
    alt op {
      box(mt) { if mt == mut { ret "@mutable "; } ret "@"; }
      uniq(mt) { if mt == mut { ret "~mutable "; } ret "~"; }
      deref. { ret "*"; }
      not. { ret "!"; }
      neg. { ret "-"; }
    }
}

fn is_path(e: @expr) -> bool {
    ret alt e.node { expr_path(_) { true } _ { false } };
}

fn ty_mach_to_str(tm: ty_mach) -> str {
    alt tm {
      ty_u8. { ret "u8"; }
      ty_u16. { ret "u16"; }
      ty_u32. { ret "u32"; }
      ty_u64. { ret "u64"; }
      ty_i8. { ret "i8"; }
      ty_i16. { ret "i16"; }
      ty_i32. { ret "i32"; }
      ty_i64. { ret "i64"; }
      ty_f32. { ret "f32"; }
      ty_f64. { ret "f64"; }
    }
}


fn is_exported(i: ident, m: _mod) -> bool {
    let nonlocal = true;
    for it: @item in m.items {
        if it.ident == i { nonlocal = false; }
        alt it.node {
          item_tag(variants, _) {
            for v: variant in variants {
                if v.node.name == i { nonlocal = false; }
            }
          }
          _ { }
        }
        if !nonlocal { break; }
    }
    let count = 0u;
    for vi: @view_item in m.view_items {
        alt vi.node {
          view_item_export(ids, _) {
            for id in ids { if str::eq(i, id) { ret true; } }
            count += 1u;
          }
          _ {/* fall through */ }
        }
    }
    // If there are no declared exports then
    // everything not imported is exported
    // even if it's nonlocal (since it's explicit)
    ret count == 0u && !nonlocal;
}

pure fn is_call_expr(e: @expr) -> bool {
    alt e.node { expr_call(_, _, _) { true } _ { false } }
}

fn is_constraint_arg(e: @expr) -> bool {
    alt e.node {
      expr_lit(_) { ret true; }
      expr_path(_) { ret true; }
      _ { ret false; }
    }
}

fn eq_ty(&&a: @ty, &&b: @ty) -> bool { ret std::box::ptr_eq(a, b); }

fn hash_ty(&&t: @ty) -> uint { ret t.span.lo << 16u + t.span.hi; }

fn hash_def_id(&&id: def_id) -> uint {
    id.crate as uint << 16u + (id.node as uint)
}

fn eq_def_id(&&a: def_id, &&b: def_id) -> bool {
    a == b
}

fn new_def_id_hash<copy T>() -> std::map::hashmap<def_id, T> {
    std::map::mk_hashmap(hash_def_id, eq_def_id)
}

fn block_from_expr(e: @expr) -> blk {
    let blk_ = default_block([], option::some::<@expr>(e), e.id);
    ret {node: blk_, span: e.span};
}

fn default_block(stmts1: [@stmt], expr1: option::t<@expr>, id1: node_id) ->
   blk_ {
    {view_items: [], stmts: stmts1, expr: expr1, id: id1, rules: default_blk}
}

fn obj_field_from_anon_obj_field(f: anon_obj_field) -> obj_field {
    ret {mut: f.mut, ty: f.ty, ident: f.ident, id: f.id};
}

// This is a convenience function to transfor ternary expressions to if
// expressions so that they can be treated the same
fn ternary_to_if(e: @expr) -> @expr {
    alt e.node {
      expr_ternary(cond, then, els) {
        let then_blk = block_from_expr(then);
        let els_blk = block_from_expr(els);
        let els_expr =
            @{id: els.id, node: expr_block(els_blk), span: els.span};
        ret @{id: e.id,
              node: expr_if(cond, then_blk, option::some(els_expr)),
              span: e.span};
      }
      _ { fail; }
    }
}

fn ty_param_kind(tp: ty_param) -> kind { tp.kind }

fn compare_lit(a: @lit, b: @lit) -> int {
    fn cmp<T>(a: T, b: T) -> int { a == b ? 0 : a < b ? -1 : 1 }
    alt (a.node, b.node) {
      (lit_int(a), lit_int(b)) |
      (lit_mach_int(_, a), lit_mach_int(_, b)) { cmp(a, b) }
      (lit_uint(a), lit_uint(b)) { cmp(a, b) }
      (lit_char(a), lit_char(b)) { cmp(a, b) }
      (lit_float(a), lit_float(b)) |
      (lit_mach_float(_, a), lit_mach_float(_, b)) {
        cmp(std::float::from_str(a), std::float::from_str(b))
      }
      (lit_str(a), lit_str(b)) { cmp(a, b) }
      (lit_nil., lit_nil.) { 0 }
      (lit_bool(a), lit_bool(b)) { cmp(a, b) }
    }
}

fn lit_eq(a: @lit, b: @lit) -> bool { compare_lit(a, b) == 0 }

fn lit_types_match(a: @lit, b: @lit) -> bool {
    alt (a.node, b.node) {
      (lit_int(_), lit_int(_)) | (lit_uint(_), lit_uint(_)) |
      (lit_char(_), lit_char(_)) | (lit_float(_), lit_float(_)) |
      (lit_str(_), lit_str(_)) | (lit_nil., lit_nil.) |
      (lit_bool(_), lit_bool(_ )) { true }
      (lit_mach_int(ta, _), lit_mach_int(tb, _)) |
      (lit_mach_float(ta, _), lit_mach_float(tb, _)) { ta == tb }
      _ { false }
    }
}

fn lit_is_numeric(l: @ast::lit) -> bool {
    alt l.node {
      ast::lit_int(_) | ast::lit_char(_) | ast::lit_uint(_) |
      ast::lit_mach_int(_, _) | ast::lit_float(_) | ast::lit_mach_float(_,_) {
        true
      }
      _ { false }
    }
}

// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
