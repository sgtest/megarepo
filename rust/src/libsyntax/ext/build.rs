import codemap::span;
import base::ext_ctxt;

fn mk_expr(cx: ext_ctxt, sp: codemap::span, expr: ast::expr_) ->
    @ast::expr {
    return @{id: cx.next_id(), callee_id: cx.next_id(),
          node: expr, span: sp};
}

fn mk_lit(cx: ext_ctxt, sp: span, lit: ast::lit_) -> @ast::expr {
    let sp_lit = @{node: lit, span: sp};
    mk_expr(cx, sp, ast::expr_lit(sp_lit))
}
fn mk_int(cx: ext_ctxt, sp: span, i: int) -> @ast::expr {
    let lit = ast::lit_int(i as i64, ast::ty_i);
    return mk_lit(cx, sp, lit);
}
fn mk_uint(cx: ext_ctxt, sp: span, u: uint) -> @ast::expr {
    let lit = ast::lit_uint(u as u64, ast::ty_u);
    return mk_lit(cx, sp, lit);
}
fn mk_u8(cx: ext_ctxt, sp: span, u: u8) -> @ast::expr {
    let lit = ast::lit_uint(u as u64, ast::ty_u8);
    return mk_lit(cx, sp, lit);
}
fn mk_binary(cx: ext_ctxt, sp: span, op: ast::binop,
             lhs: @ast::expr, rhs: @ast::expr)
   -> @ast::expr {
    cx.next_id(); // see ast_util::op_expr_callee_id
    mk_expr(cx, sp, ast::expr_binary(op, lhs, rhs))
}
fn mk_unary(cx: ext_ctxt, sp: span, op: ast::unop, e: @ast::expr)
    -> @ast::expr {
    cx.next_id(); // see ast_util::op_expr_callee_id
    mk_expr(cx, sp, ast::expr_unary(op, e))
}
fn mk_path(cx: ext_ctxt, sp: span, idents: ~[ast::ident]) ->
    @ast::expr {
    let path = @{span: sp, global: false, idents: idents,
                 rp: None, types: ~[]};
    let pathexpr = ast::expr_path(path);
    mk_expr(cx, sp, pathexpr)
}
fn mk_access_(cx: ext_ctxt, sp: span, p: @ast::expr, m: ast::ident)
    -> @ast::expr {
    mk_expr(cx, sp, ast::expr_field(p, m, ~[]))
}
fn mk_access(cx: ext_ctxt, sp: span, p: ~[ast::ident], m: ast::ident)
    -> @ast::expr {
    let pathexpr = mk_path(cx, sp, p);
    return mk_access_(cx, sp, pathexpr, m);
}
fn mk_call_(cx: ext_ctxt, sp: span, fn_expr: @ast::expr,
            args: ~[@ast::expr]) -> @ast::expr {
    mk_expr(cx, sp, ast::expr_call(fn_expr, args, false))
}
fn mk_call(cx: ext_ctxt, sp: span, fn_path: ~[ast::ident],
             args: ~[@ast::expr]) -> @ast::expr {
    let pathexpr = mk_path(cx, sp, fn_path);
    return mk_call_(cx, sp, pathexpr, args);
}
// e = expr, t = type
fn mk_base_vec_e(cx: ext_ctxt, sp: span, exprs: ~[@ast::expr]) ->
   @ast::expr {
    let vecexpr = ast::expr_vec(exprs, ast::m_imm);
    mk_expr(cx, sp, vecexpr)
}
fn mk_vstore_e(cx: ext_ctxt, sp: span, expr: @ast::expr, vst: ast::vstore) ->
   @ast::expr {
    mk_expr(cx, sp, ast::expr_vstore(expr, vst))
}
fn mk_uniq_vec_e(cx: ext_ctxt, sp: span, exprs: ~[@ast::expr]) ->
   @ast::expr {
    mk_vstore_e(cx, sp, mk_base_vec_e(cx, sp, exprs), ast::vstore_uniq)
}
fn mk_fixed_vec_e(cx: ext_ctxt, sp: span, exprs: ~[@ast::expr]) ->
   @ast::expr {
    mk_vstore_e(cx, sp, mk_base_vec_e(cx, sp, exprs), ast::vstore_fixed(None))
}
fn mk_base_str(cx: ext_ctxt, sp: span, s: ~str) -> @ast::expr {
    let lit = ast::lit_str(@s);
    return mk_lit(cx, sp, lit);
}
fn mk_uniq_str(cx: ext_ctxt, sp: span, s: ~str) -> @ast::expr {
    mk_vstore_e(cx, sp, mk_base_str(cx, sp, s), ast::vstore_uniq)
}

fn mk_rec_e(cx: ext_ctxt, sp: span,
            fields: ~[{ident: ast::ident, ex: @ast::expr}]) ->
    @ast::expr {
    let mut astfields: ~[ast::field] = ~[];
    for fields.each |field| {
        let ident = field.ident;
        let val = field.ex;
        let astfield =
            {node: {mutbl: ast::m_imm, ident: ident, expr: val}, span: sp};
        vec::push(astfields, astfield);
    }
    let recexpr = ast::expr_rec(astfields, option::None::<@ast::expr>);
    mk_expr(cx, sp, recexpr)
}

