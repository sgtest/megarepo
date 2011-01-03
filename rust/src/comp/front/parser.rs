import std._io;
import std.option;
import std.option.some;
import std.option.none;
import std.map.hashmap;

import driver.session;
import util.common;
import util.common.append;
import util.common.span;
import util.common.new_str_hash;

state type parser =
    state obj {
          fn peek() -> token.token;
          impure fn bump();
          impure fn err(str s);
          impure fn restrict(bool r);
          fn is_restricted() -> bool;
          fn get_session() -> session.session;
          fn get_span() -> common.span;
          fn next_def_id() -> ast.def_id;
    };

impure fn new_parser(session.session sess,
                 ast.crate_num crate, str path) -> parser {
    state obj stdio_parser(session.session sess,
                           mutable token.token tok,
                           mutable common.pos lo,
                           mutable common.pos hi,
                           mutable ast.def_num def,
                           mutable bool restricted,
                           ast.crate_num crate,
                           lexer.reader rdr)
        {
            fn peek() -> token.token {
                // log token.to_str(tok);
                ret tok;
            }

            impure fn bump() {
                tok = lexer.next_token(rdr);
                lo = rdr.get_mark_pos();
                hi = rdr.get_curr_pos();
            }

            impure fn err(str m) {
                auto span = rec(filename = rdr.get_filename(),
                                lo = lo, hi = hi);
                sess.span_err(span, m);
            }

            impure fn restrict(bool r) {
                restricted = r;
            }

            fn is_restricted() -> bool {
                ret restricted;
            }

            fn get_session() -> session.session {
                ret sess;
            }

            fn get_span() -> common.span {
                ret rec(filename = rdr.get_filename(),
                        lo = lo, hi = hi);
            }

            fn next_def_id() -> ast.def_id {
                def += 1;
                ret tup(crate, def);
            }
        }
    auto srdr = _io.new_stdio_reader(path);
    auto rdr = lexer.new_reader(srdr, path);
    auto npos = rdr.get_curr_pos();
    ret stdio_parser(sess, lexer.next_token(rdr),
                     npos, npos, 0, false, crate, rdr);
}

impure fn unexpected(parser p, token.token t) {
    let str s = "unexpected token: ";
    s += token.to_str(t);
    p.err(s);
}

impure fn expect(parser p, token.token t) {
    if (p.peek() == t) {
        p.bump();
    } else {
        let str s = "expecting ";
        s += token.to_str(t);
        s += ", found ";
        s += token.to_str(p.peek());
        p.err(s);
    }
}

fn spanned[T](&span lo, &span hi, &T node) -> ast.spanned[T] {
    ret rec(node=node, span=rec(filename=lo.filename,
                                lo=lo.lo,
                                hi=hi.hi));
}

impure fn parse_ident(parser p) -> ast.ident {
    alt (p.peek()) {
        case (token.IDENT(?i)) { p.bump(); ret i; }
        case (_) {
            p.err("expecting ident");
            fail;
        }
    }
}

impure fn parse_ty_fn(parser p, ast.span lo) -> ast.ty_ {
    impure fn parse_fn_input_ty(parser p) -> rec(ast.mode mode, @ast.ty ty) {
        auto mode;
        if (p.peek() == token.BINOP(token.AND)) {
            p.bump();
            mode = ast.alias;
        } else {
            mode = ast.val;
        }

        auto t = parse_ty(p);

        alt (p.peek()) {
            case (token.IDENT(_)) { p.bump(); /* ignore the param name */ }
            case (_) { /* no param name present */ }
        }

        ret rec(mode=mode, ty=t);
    }

    auto lo = p.get_span();

    auto f = parse_fn_input_ty; // FIXME: trans_const_lval bug
    auto inputs = parse_seq[rec(ast.mode mode, @ast.ty ty)](token.LPAREN,
        token.RPAREN, some(token.COMMA), f, p);

    let @ast.ty output;
    if (p.peek() == token.RARROW) {
        p.bump();
        output = parse_ty(p);
    } else {
        output = @spanned(lo, inputs.span, ast.ty_nil);
    }

    ret ast.ty_fn(inputs.node, output);
}

impure fn parse_ty_obj(parser p, &mutable ast.span hi) -> ast.ty_ {
    expect(p, token.OBJ);
    impure fn parse_method_sig(parser p) -> ast.ty_method {
        auto flo = p.get_span();
        expect(p, token.FN);
        auto ident = parse_ident(p);
        auto f = parse_ty_fn(p, flo);
        expect(p, token.SEMI);
        alt (f) {
            case (ast.ty_fn(?inputs, ?output)) {
                ret rec(ident=ident, inputs=inputs, output=output);
            }
        }
        fail;
    }
    auto f = parse_method_sig;
    auto meths =
        parse_seq[ast.ty_method](token.LBRACE,
                                 token.RBRACE,
                                 none[token.token],
                                 f, p);
    hi = meths.span;
    ret ast.ty_obj(meths.node);
}

impure fn parse_ty_field(parser p) -> ast.ty_field {
    auto ty = parse_ty(p);
    auto id = parse_ident(p);
    ret rec(ident=id, ty=ty);
}

impure fn parse_ty(parser p) -> @ast.ty {
    auto lo = p.get_span();
    auto hi = lo;
    let ast.ty_ t;
    alt (p.peek()) {
        case (token.BOOL) { p.bump(); t = ast.ty_bool; }
        case (token.INT) { p.bump(); t = ast.ty_int; }
        case (token.UINT) { p.bump(); t = ast.ty_int; }
        case (token.STR) { p.bump(); t = ast.ty_str; }
        case (token.CHAR) { p.bump(); t = ast.ty_char; }
        case (token.MACH(?tm)) { p.bump(); t = ast.ty_machine(tm); }

        case (token.LPAREN) {
            p.bump();
            alt (p.peek()) {
                case (token.RPAREN) {
                    hi = p.get_span();
                    p.bump();
                    t = ast.ty_nil;
                }
                case (_) {
                    t = parse_ty(p).node;
                    hi = p.get_span();
                    expect(p, token.RPAREN);
                }
            }
        }

        case (token.AT) {
            p.bump();
            auto t0 = parse_ty(p);
            hi = t0.span;
            t = ast.ty_box(t0);
        }

        case (token.VEC) {
            p.bump();
            expect(p, token.LBRACKET);
            t = ast.ty_vec(parse_ty(p));
            hi = p.get_span();
            expect(p, token.RBRACKET);
        }

        case (token.TUP) {
            p.bump();
            auto f = parse_ty; // FIXME: trans_const_lval bug
            auto elems = parse_seq[@ast.ty] (token.LPAREN,
                                             token.RPAREN,
                                             some(token.COMMA), f, p);
            hi = elems.span;
            t = ast.ty_tup(elems.node);
        }

        case (token.REC) {
            p.bump();
            auto f = parse_ty_field; // FIXME: trans_const_lval bug
            auto elems =
                parse_seq[ast.ty_field](token.LPAREN,
                                        token.RPAREN,
                                        some(token.COMMA),
                                        f, p);
            hi = elems.span;
            t = ast.ty_rec(elems.node);
        }

        case (token.MUTABLE) {
            p.bump();
            auto t0 = parse_ty(p);
            hi = t0.span;
            t = ast.ty_mutable(t0);
        }

        case (token.FN) {
            auto flo = p.get_span();
            p.bump();
            t = parse_ty_fn(p, flo);
            alt (t) {
                case (ast.ty_fn(_, ?out)) {
                    hi = out.span;
                }
            }
        }

        case (token.OBJ) {
            t = parse_ty_obj(p, hi);
        }

        case (token.IDENT(_)) {
            let ast.path pth = vec();
            let bool more = true;
            while (more) {
                alt (p.peek()) {
                    case (token.IDENT(?i)) {
                        auto n = parse_name(p, i);
                        hi = n.span;
                        pth += n;
                        if (p.peek() == token.DOT) {
                            p.bump();
                        } else {
                            more = false;
                        }
                    }
                    case (_) {
                        more = false;
                    }
                }
            }
            t = ast.ty_path(pth, none[ast.def]);
        }

        case (_) {
            p.err("expecting type");
            t = ast.ty_nil;
            fail;
        }
    }
    ret @spanned(lo, hi, t);
}

impure fn parse_arg(parser p) -> ast.arg {
    let ast.mode m = ast.val;
    if (p.peek() == token.BINOP(token.AND)) {
        m = ast.alias;
        p.bump();
    }
    let @ast.ty t = parse_ty(p);
    let ast.ident i = parse_ident(p);
    ret rec(mode=m, ty=t, ident=i, id=p.next_def_id());
}

impure fn parse_seq[T](token.token bra,
                      token.token ket,
                      option.t[token.token] sep,
                      (impure fn(parser) -> T) f,
                      parser p) -> util.common.spanned[vec[T]] {
    let bool first = true;
    auto lo = p.get_span();
    expect(p, bra);
    let vec[T] v = vec();
    while (p.peek() != ket) {
        alt(sep) {
            case (some[token.token](?t)) {
                if (first) {
                    first = false;
                } else {
                    expect(p, t);
                }
            }
            case (_) {
            }
        }
        // FIXME: v += f(p) doesn't work at the moment.
        let T t = f(p);
        v += vec(t);
    }
    auto hi = p.get_span();
    expect(p, ket);
    ret spanned(lo, hi, v);
}

impure fn parse_lit(parser p) -> option.t[ast.lit] {
    auto lo = p.get_span();
    let ast.lit_ lit;
    alt (p.peek()) {
        case (token.LIT_INT(?i)) {
            p.bump();
            lit = ast.lit_int(i);
        }
        case (token.LIT_UINT(?u)) {
            p.bump();
            lit = ast.lit_uint(u);
        }
        case (token.LIT_MACH_INT(?tm, ?i)) {
            p.bump();
            lit = ast.lit_mach_int(tm, i);
        }
        case (token.LIT_CHAR(?c)) {
            p.bump();
            lit = ast.lit_char(c);
        }
        case (token.LIT_BOOL(?b)) {
            p.bump();
            lit = ast.lit_bool(b);
        }
        case (token.LIT_STR(?s)) {
            p.bump();
            lit = ast.lit_str(s);
        }
        case (_) {
            lit = ast.lit_nil;  // FIXME: typestate bug requires this
            ret none[ast.lit];
        }
    }
    ret some(spanned(lo, lo, lit));
}

impure fn parse_name(parser p, ast.ident id) -> ast.name {

    auto lo = p.get_span();

    p.bump();

    let vec[@ast.ty] v = vec();
    let util.common.spanned[vec[@ast.ty]] tys = rec(node=v, span=lo);

    alt (p.peek()) {
        case (token.LBRACKET) {
            auto pf = parse_ty;
            tys = parse_seq[@ast.ty](token.LBRACKET,
                                     token.RBRACKET,
                                     some(token.COMMA),
                                     pf, p);
        }
        case (_) {
        }
    }
    ret spanned(lo, tys.span, rec(ident=id, types=tys.node));
}

impure fn parse_mutabliity(parser p) -> ast.mutability {
    if (p.peek() == token.MUTABLE) {
        p.bump();
        ret ast.mut;
    }
    ret ast.imm;
}

impure fn parse_field(parser p) -> ast.field {
    auto m = parse_mutabliity(p);
    auto i = parse_ident(p);
    expect(p, token.EQ);
    auto e = parse_expr(p);
    ret rec(mut=m, ident=i, expr=e);
}

impure fn parse_bottom_expr(parser p) -> @ast.expr {

    auto lo = p.get_span();
    auto hi = lo;

    // FIXME: can only remove this sort of thing when both typestate and
    // alt-exhaustive-match checking are co-operating.
    auto lit = @spanned(lo, lo, ast.lit_nil);
    let ast.expr_ ex = ast.expr_lit(lit, ast.ann_none);

    alt (p.peek()) {

        case (token.IDENT(?i)) {
            auto n = parse_name(p, i);
            hi = n.span;
            ex = ast.expr_name(n, none[ast.def], ast.ann_none);
        }

        case (token.LPAREN) {
            p.bump();
            alt (p.peek()) {
                case (token.RPAREN) {
                    hi = p.get_span();
                    p.bump();
                    auto lit = @spanned(lo, hi, ast.lit_nil);
                    ret @spanned(lo, hi,
                                 ast.expr_lit(lit, ast.ann_none));
                }
                case (_) { /* fall through */ }
            }
            auto e = parse_expr(p);
            hi = p.get_span();
            expect(p, token.RPAREN);
            ret @spanned(lo, hi, e.node);
        }

        case (token.TUP) {
            p.bump();
            impure fn parse_elt(parser p) -> ast.elt {
                auto m = parse_mutabliity(p);
                auto e = parse_expr(p);
                ret rec(mut=m, expr=e);
            }
            auto pf = parse_elt;
            auto es =
                parse_seq[ast.elt](token.LPAREN,
                                   token.RPAREN,
                                   some(token.COMMA),
                                   pf, p);
            hi = es.span;
            ex = ast.expr_tup(es.node, ast.ann_none);
        }

        case (token.VEC) {
            p.bump();
            auto pf = parse_expr;
            auto es = parse_seq[@ast.expr](token.LPAREN,
                                           token.RPAREN,
                                           some(token.COMMA),
                                           pf, p);
            hi = es.span;
            ex = ast.expr_vec(es.node, ast.ann_none);
        }

        case (token.REC) {
            p.bump();
            auto pf = parse_field;
            auto fs =
                parse_seq[ast.field](token.LPAREN,
                                     token.RPAREN,
                                     some(token.COMMA),
                                     pf, p);
            hi = fs.span;
            ex = ast.expr_rec(fs.node, ast.ann_none);
        }

        case (token.BIND) {
            p.bump();
            auto e = parse_restricted_expr(p);
            impure fn parse_expr_opt(parser p) -> option.t[@ast.expr] {
                alt (p.peek()) {
                    case (token.UNDERSCORE) {
                        p.bump();
                        ret none[@ast.expr];
                    }
                    case (_) {
                        ret some[@ast.expr](parse_expr(p));
                    }
                }
            }

            auto pf = parse_expr_opt;
            auto es = parse_seq[option.t[@ast.expr]](token.LPAREN,
                                                     token.RPAREN,
                                                     some(token.COMMA),
                                                     pf, p);
            hi = es.span;
            auto e_ = ast.expr_bind(e, es.node, ast.ann_none);
            e = @spanned(lo, hi, e_);
        }

        case (_) {
            alt (parse_lit(p)) {
                case (some[ast.lit](?lit)) {
                    hi = lit.span;
                    ex = ast.expr_lit(@lit, ast.ann_none);
                }
                case (none[ast.lit]) {
                    p.err("expecting expression");
                }
            }
        }
    }

    ret @spanned(lo, hi, ex);
}

impure fn parse_path_expr(parser p) -> @ast.expr {
    auto lo = p.get_span();
    auto e = parse_bottom_expr(p);
    auto hi = e.span;
    while (true) {
        alt (p.peek()) {

            case (token.LPAREN) {
                if (p.is_restricted()) {
                    ret e;
                } else {
                    // Call expr.
                    auto pf = parse_expr;
                    auto es = parse_seq[@ast.expr](token.LPAREN,
                                                   token.RPAREN,
                                                   some(token.COMMA),
                                                   pf, p);
                    hi = es.span;
                    auto e_ = ast.expr_call(e, es.node, ast.ann_none);
                    e = @spanned(lo, hi, e_);
                }
            }

            case (token.DOT) {
                p.bump();
                alt (p.peek()) {

                    case (token.IDENT(?i)) {
                        hi = p.get_span();
                        p.bump();
                        auto e_ = ast.expr_field(e, i, ast.ann_none);
                        e = @spanned(lo, hi, e_);
                    }

                    case (token.LPAREN) {
                        p.bump();
                        auto ix = parse_expr(p);
                        hi = ix.span;
                        expect(p, token.RPAREN);
                        auto e_ = ast.expr_index(e, ix, ast.ann_none);
                        e = @spanned(lo, hi, e_);
                    }

                    case (?t) {
                        unexpected(p, t);
                    }
                }
            }
            case (_) {
                ret e;
            }
        }
    }
    ret e;
}

impure fn parse_prefix_expr(parser p) -> @ast.expr {

    auto lo = p.get_span();
    auto hi = lo;

    // FIXME: can only remove this sort of thing when both typestate and
    // alt-exhaustive-match checking are co-operating.
    auto lit = @spanned(lo, lo, ast.lit_nil);
    let ast.expr_ ex = ast.expr_lit(lit, ast.ann_none);

    alt (p.peek()) {

        case (token.NOT) {
            p.bump();
            auto e = parse_prefix_expr(p);
            hi = e.span;
            ex = ast.expr_unary(ast.not, e, ast.ann_none);
        }

        case (token.TILDE) {
            p.bump();
            auto e = parse_prefix_expr(p);
            hi = e.span;
            ex = ast.expr_unary(ast.bitnot, e, ast.ann_none);
        }

        case (token.BINOP(?b)) {
            alt (b) {
                case (token.MINUS) {
                    p.bump();
                    auto e = parse_prefix_expr(p);
                    hi = e.span;
                    ex = ast.expr_unary(ast.neg, e, ast.ann_none);
                }

                case (token.STAR) {
                    p.bump();
                    auto e = parse_prefix_expr(p);
                    hi = e.span;
                    ex = ast.expr_unary(ast.deref, e, ast.ann_none);
                }

                case (_) {
                    ret parse_path_expr(p);
                }
            }
        }

        case (token.AT) {
            p.bump();
            auto e = parse_prefix_expr(p);
            hi = e.span;
            ex = ast.expr_unary(ast.box, e, ast.ann_none);
        }

        case (_) {
            ret parse_path_expr(p);
        }
    }
    ret @spanned(lo, hi, ex);
}

impure fn parse_binops(parser p,
                   (impure fn(parser) -> @ast.expr) sub,
                   vec[tup(token.binop, ast.binop)] ops)
    -> @ast.expr {
    auto lo = p.get_span();
    auto hi = lo;
    auto e = sub(p);
    auto more = true;
    while (more) {
        more = false;
        for (tup(token.binop, ast.binop) pair in ops) {
            alt (p.peek()) {
                case (token.BINOP(?op)) {
                    if (pair._0 == op) {
                        p.bump();
                        auto rhs = sub(p);
                        hi = rhs.span;
                        auto exp = ast.expr_binary(pair._1, e, rhs,
                                                   ast.ann_none);
                        e = @spanned(lo, hi, exp);
                        more = true;
                    }
                }
                case (_) { /* fall through */ }
            }
        }
    }
    ret e;
}

impure fn parse_binary_exprs(parser p,
                            (impure fn(parser) -> @ast.expr) sub,
                            vec[tup(token.token, ast.binop)] ops)
    -> @ast.expr {
    auto lo = p.get_span();
    auto hi = lo;
    auto e = sub(p);
    auto more = true;
    while (more) {
        more = false;
        for (tup(token.token, ast.binop) pair in ops) {
            if (pair._0 == p.peek()) {
                p.bump();
                auto rhs = sub(p);
                hi = rhs.span;
                auto exp = ast.expr_binary(pair._1, e, rhs, ast.ann_none);
                e = @spanned(lo, hi, exp);
                more = true;
            }
        }
    }
    ret e;
}

impure fn parse_factor_expr(parser p) -> @ast.expr {
    auto sub = parse_prefix_expr;
    ret parse_binops(p, sub, vec(tup(token.STAR, ast.mul),
                                 tup(token.SLASH, ast.div),
                                 tup(token.PERCENT, ast.rem)));
}

impure fn parse_term_expr(parser p) -> @ast.expr {
    auto sub = parse_factor_expr;
    ret parse_binops(p, sub, vec(tup(token.PLUS, ast.add),
                                 tup(token.MINUS, ast.sub)));
}

impure fn parse_shift_expr(parser p) -> @ast.expr {
    auto sub = parse_term_expr;
    ret parse_binops(p, sub, vec(tup(token.LSL, ast.lsl),
                                 tup(token.LSR, ast.lsr),
                                 tup(token.ASR, ast.asr)));
}

impure fn parse_bitand_expr(parser p) -> @ast.expr {
    auto sub = parse_shift_expr;
    ret parse_binops(p, sub, vec(tup(token.AND, ast.bitand)));
}

impure fn parse_bitxor_expr(parser p) -> @ast.expr {
    auto sub = parse_bitand_expr;
    ret parse_binops(p, sub, vec(tup(token.CARET, ast.bitxor)));
}

impure fn parse_bitor_expr(parser p) -> @ast.expr {
    auto sub = parse_bitxor_expr;
    ret parse_binops(p, sub, vec(tup(token.OR, ast.bitor)));
}

impure fn parse_cast_expr(parser p) -> @ast.expr {
    auto lo = p.get_span();
    auto e = parse_bitor_expr(p);
    auto hi = e.span;
    while (true) {
        alt (p.peek()) {
            case (token.AS) {
                p.bump();
                auto t = parse_ty(p);
                hi = t.span;
                e = @spanned(lo, hi, ast.expr_cast(e, t, ast.ann_none));
            }

            case (_) {
                ret e;
            }
        }
    }
    ret e;
}

impure fn parse_relational_expr(parser p) -> @ast.expr {
    auto sub = parse_cast_expr;
    ret parse_binary_exprs(p, sub, vec(tup(token.LT, ast.lt),
                                       tup(token.LE, ast.le),
                                       tup(token.GE, ast.ge),
                                       tup(token.GT, ast.gt)));
}


impure fn parse_equality_expr(parser p) -> @ast.expr {
    auto sub = parse_relational_expr;
    ret parse_binary_exprs(p, sub, vec(tup(token.EQEQ, ast.eq),
                                       tup(token.NE, ast.ne)));
}

impure fn parse_and_expr(parser p) -> @ast.expr {
    auto sub = parse_equality_expr;
    ret parse_binary_exprs(p, sub, vec(tup(token.ANDAND, ast.and)));
}

impure fn parse_or_expr(parser p) -> @ast.expr {
    auto sub = parse_and_expr;
    ret parse_binary_exprs(p, sub, vec(tup(token.OROR, ast.or)));
}

impure fn parse_assign_expr(parser p) -> @ast.expr {
    auto lo = p.get_span();
    auto lhs = parse_or_expr(p);
    alt (p.peek()) {
        case (token.EQ) {
            p.bump();
            auto rhs = parse_expr(p);
            ret @spanned(lo, rhs.span,
                         ast.expr_assign(lhs, rhs, ast.ann_none));
        }
        case (token.BINOPEQ(?op)) {
            p.bump();
            auto rhs = parse_expr(p);
            auto aop = ast.add;
            alt (op) {
                case (token.PLUS) { aop = ast.add; }
                case (token.MINUS) { aop = ast.sub; }
                case (token.STAR) { aop = ast.mul; }
                case (token.SLASH) { aop = ast.div; }
                case (token.PERCENT) { aop = ast.rem; }
                case (token.CARET) { aop = ast.bitxor; }
                case (token.AND) { aop = ast.bitand; }
                case (token.OR) { aop = ast.bitor; }
                case (token.LSL) { aop = ast.lsl; }
                case (token.LSR) { aop = ast.lsr; }
                case (token.ASR) { aop = ast.asr; }
            }
            ret @spanned(lo, rhs.span,
                         ast.expr_assign_op(aop, lhs, rhs, ast.ann_none));
        }
        case (_) { /* fall through */ }
    }
    ret lhs;
}

impure fn parse_if_expr(parser p) -> @ast.expr {
    auto lo = p.get_span();
    auto hi = lo;

    expect(p, token.IF);
    expect(p, token.LPAREN);
    auto cond = parse_expr(p);
    expect(p, token.RPAREN);
    auto thn = parse_block(p);
    let option.t[ast.block] els = none[ast.block];
    hi = thn.span;
    alt (p.peek()) {
        case (token.ELSE) {
            p.bump();
            auto eblk = parse_block(p);
            els = some(eblk);
            hi = eblk.span;
        }
        case (_) { /* fall through */ }
    }
    ret @spanned(lo, hi, ast.expr_if(cond, thn, els, ast.ann_none));
}

impure fn parse_while_expr(parser p) -> @ast.expr {
    auto lo = p.get_span();
    auto hi = lo;

    expect(p, token.WHILE);
    expect (p, token.LPAREN);
    auto cond = parse_expr(p);
    expect(p, token.RPAREN);
    auto body = parse_block(p);
    hi = body.span;
    ret @spanned(lo, hi, ast.expr_while(cond, body, ast.ann_none));
}

impure fn parse_do_while_expr(parser p) -> @ast.expr {
    auto lo = p.get_span();
    auto hi = lo;

    expect(p, token.DO);
    auto body = parse_block(p);
    expect(p, token.WHILE);
    expect (p, token.LPAREN);
    auto cond = parse_expr(p);
    expect(p, token.RPAREN);
    hi = cond.span;
    ret @spanned(lo, hi, ast.expr_do_while(body, cond, ast.ann_none));
}

impure fn parse_alt_expr(parser p) -> @ast.expr {
    auto lo = p.get_span();
    expect(p, token.ALT);
    expect(p, token.LPAREN);
    auto discriminant = parse_expr(p);
    expect(p, token.RPAREN);
    expect(p, token.LBRACE);

    let vec[ast.arm] arms = vec();
    while (p.peek() != token.RBRACE) {
        alt (p.peek()) {
            case (token.CASE) {
                p.bump();
                expect(p, token.LPAREN);
                auto pat = parse_pat(p);
                expect(p, token.RPAREN);
                auto index = index_arm(pat);
                auto block = parse_block(p);
                arms += vec(rec(pat=pat, block=block, index=index));
            }
            case (token.RBRACE) { /* empty */ }
            case (?tok) {
                p.err("expected 'case' or '}' when parsing 'alt' statement " +
                      "but found " + token.to_str(tok));
            }
        }
    }
    p.bump();

    auto expr = ast.expr_alt(discriminant, arms, ast.ann_none);
    auto hi = p.get_span();
    ret @spanned(lo, hi, expr);
}


impure fn parse_restricted_expr(parser p) -> @ast.expr {
    ret parse_expr_res(p, true);
}

impure fn parse_expr(parser p) -> @ast.expr {
    ret parse_expr_res(p, false);
}

impure fn parse_expr_res(parser p, bool restrict) -> @ast.expr {
    auto old = p.is_restricted();
    p.restrict(restrict);
    auto e = parse_expr_inner(p);
    p.restrict(old);
    ret e;
}

impure fn parse_expr_inner(parser p) -> @ast.expr {
    alt (p.peek()) {
        case (token.LBRACE) {
            auto blk = parse_block(p);
            ret @spanned(blk.span, blk.span,
                         ast.expr_block(blk, ast.ann_none));
        }
        case (token.IF) {
            ret parse_if_expr(p);
        }
        case (token.WHILE) {
            ret parse_while_expr(p);
        }
        case (token.DO) {
            ret parse_do_while_expr(p);
        }
        case (token.ALT) {
            ret parse_alt_expr(p);
        }
        case (_) {
            ret parse_assign_expr(p);
        }

    }
}

impure fn parse_initializer(parser p) -> option.t[@ast.expr] {
    if (p.peek() == token.EQ) {
        p.bump();
        ret some(parse_expr(p));
    }

    ret none[@ast.expr];
}

impure fn parse_pat(parser p) -> @ast.pat {
    auto lo = p.get_span();

    auto pat = ast.pat_wild(ast.ann_none);  // FIXME: typestate bug
    alt (p.peek()) {
        case (token.UNDERSCORE) {
            p.bump();
            pat = ast.pat_wild(ast.ann_none);
        }
        case (token.QUES) {
            p.bump();
            alt (p.peek()) {
                case (token.IDENT(?id)) {
                    p.bump();
                    pat = ast.pat_bind(id, p.next_def_id(), ast.ann_none);
                }
                case (?tok) {
                    p.err("expected identifier after '?' in pattern but " +
                          "found " + token.to_str(tok));
                    fail;
                }
            }
        }
        case (token.IDENT(?id)) {
            p.bump();

            let vec[@ast.pat] args;
            alt (p.peek()) {
                case (token.LPAREN) {
                    auto f = parse_pat;
                    args = parse_seq[@ast.pat](token.LPAREN, token.RPAREN,
                                               some(token.COMMA), f, p).node;
                }
                case (_) { args = vec(); }
            }

            pat = ast.pat_tag(id, args, none[ast.variant_def], ast.ann_none);
        }
        case (?tok) {
            p.err("expected pattern but found " + token.to_str(tok));
            fail;
        }
    }

    auto hi = p.get_span();
    ret @spanned(lo, hi, pat);
}

impure fn parse_let(parser p) -> @ast.decl {
    auto lo = p.get_span();

    expect(p, token.LET);
    auto ty = parse_ty(p);
    auto ident = parse_ident(p);
    auto init = parse_initializer(p);

    auto hi = p.get_span();

    let ast.local local = rec(ty = some(ty),
                              infer = false,
                              ident = ident,
                              init = init,
                              id = p.next_def_id(),
                              ann = ast.ann_none);

    ret @spanned(lo, hi, ast.decl_local(@local));
}

impure fn parse_auto(parser p) -> @ast.decl {
    auto lo = p.get_span();

    expect(p, token.AUTO);
    auto ident = parse_ident(p);
    auto init = parse_initializer(p);

    auto hi = p.get_span();

    let ast.local local = rec(ty = none[@ast.ty],
                              infer = true,
                              ident = ident,
                              init = init,
                              id = p.next_def_id(),
                              ann = ast.ann_none);

    ret @spanned(lo, hi, ast.decl_local(@local));
}

impure fn parse_stmt(parser p) -> @ast.stmt {
    auto lo = p.get_span();
    alt (p.peek()) {

        case (token.LOG) {
            p.bump();
            auto e = parse_expr(p);
            auto hi = p.get_span();
            ret @spanned(lo, hi, ast.stmt_log(e));
        }

        case (token.CHECK) {
            p.bump();
            alt (p.peek()) {
                case (token.LPAREN) {
                    auto e = parse_expr(p);
                    auto hi = p.get_span();
                    ret @spanned(lo, hi, ast.stmt_check_expr(e));
                }
                case (_) {
                    p.get_session().unimpl("constraint-check stmt");
                }
            }
        }

        case (token.RET) {
            p.bump();
            alt (p.peek()) {
                case (token.SEMI) {
                    ret @spanned(lo, p.get_span(),
                                 ast.stmt_ret(none[@ast.expr]));
                }
                case (_) {
                    auto e = parse_expr(p);
                    ret @spanned(lo, e.span,
                                 ast.stmt_ret(some[@ast.expr](e)));
                }
            }
        }

        case (token.LET) {
            auto decl = parse_let(p);
            auto hi = p.get_span();
            ret @spanned(lo, hi, ast.stmt_decl(decl));
        }

        case (token.AUTO) {
            auto decl = parse_auto(p);
            auto hi = p.get_span();
            ret @spanned(lo, hi, ast.stmt_decl(decl));
        }

        // Handle the (few) block-expr stmts first.

        case (token.IF) {
            auto e = parse_expr(p);
            ret @spanned(lo, e.span, ast.stmt_expr(e));
        }

        case (token.WHILE) {
            auto e = parse_expr(p);
            ret @spanned(lo, e.span, ast.stmt_expr(e));
        }

        case (token.DO) {
            auto e = parse_expr(p);
            ret @spanned(lo, e.span, ast.stmt_expr(e));
        }

        case (token.ALT) {
            auto e = parse_expr(p);
            ret @spanned(lo, e.span, ast.stmt_expr(e));
        }

        case (token.LBRACE) {
            auto e = parse_expr(p);
            ret @spanned(lo, e.span, ast.stmt_expr(e));
        }


        case (_) {
            if (peeking_at_item(p)) {
                // Might be a local item decl.
                auto i = parse_item(p);
                auto hi = i.span;
                auto decl = @spanned(lo, hi, ast.decl_item(i));
                ret @spanned(lo, hi, ast.stmt_decl(decl));

            } else {
                // Remainder are line-expr stmts.
                auto e = parse_expr(p);
                auto hi = p.get_span();
                ret @spanned(lo, hi, ast.stmt_expr(e));
            }
        }
    }
    p.err("expected statement");
    fail;
}

fn index_block(vec[@ast.stmt] stmts, option.t[@ast.expr] expr) -> ast.block_ {
    auto index = new_str_hash[uint]();
    auto u = 0u;
    for (@ast.stmt s in stmts) {
        // FIXME: typestate bug requires we do this up top, not
        // down below loop. Sigh.
        u += 1u;
        alt (s.node) {
            case (ast.stmt_decl(?d)) {
                alt (d.node) {
                    case (ast.decl_local(?loc)) {
                        index.insert(loc.ident, u-1u);
                    }
                    case (ast.decl_item(?it)) {
                        alt (it.node) {
                            case (ast.item_fn(?i, _, _, _, _)) {
                                index.insert(i, u-1u);
                            }
                            case (ast.item_mod(?i, _, _)) {
                                index.insert(i, u-1u);
                            }
                            case (ast.item_ty(?i, _, _, _, _)) {
                                index.insert(i, u-1u);
                            }
                            case (ast.item_tag(?i, _, _, _)) {
                                index.insert(i, u-1u);
                            }
                            case (ast.item_obj(?i, _, _, _, _)) {
                                index.insert(i, u-1u);
                            }
                        }
                    }
                }
            }
            case (_) { /* fall through */ }
        }
    }
    ret rec(stmts=stmts, expr=expr, index=index);
}

fn index_arm(@ast.pat pat) -> hashmap[ast.ident,ast.def_id] {
    fn do_index_arm(&hashmap[ast.ident,ast.def_id] index, @ast.pat pat) {
        alt (pat.node) {
            case (ast.pat_bind(?i, ?def_id, _)) { index.insert(i, def_id); }
            case (ast.pat_wild(_)) { /* empty */ }
            case (ast.pat_tag(_, ?pats, _, _)) {
                for (@ast.pat p in pats) {
                    do_index_arm(index, p);
                }
            }
        }
    }

    auto index = new_str_hash[ast.def_id]();
    do_index_arm(index, pat);
    ret index;
}

fn stmt_to_expr(@ast.stmt stmt) -> option.t[@ast.expr] {
    alt (stmt.node) {
        case (ast.stmt_expr(?e)) { ret some[@ast.expr](e); }
        case (_) { /* fall through */ }
    }
    ret none[@ast.expr];
}

fn stmt_ends_with_semi(@ast.stmt stmt) -> bool {
    alt (stmt.node) {
        case (ast.stmt_decl(?d)) {
            alt (d.node) {
                case (ast.decl_local(_)) { ret true; }
                case (ast.decl_item(_)) { ret false; }
            }
        }
        case (ast.stmt_ret(_))                  { ret true; }
        case (ast.stmt_log(_))                  { ret true; }
        case (ast.stmt_check_expr(_))           { ret true; }
        case (ast.stmt_expr(?e)) {
            alt (e.node) {
                case (ast.expr_vec(_,_))        { ret true; }
                case (ast.expr_tup(_,_))        { ret true; }
                case (ast.expr_rec(_,_))        { ret true; }
                case (ast.expr_call(_,_,_))     { ret true; }
                case (ast.expr_binary(_,_,_,_)) { ret true; }
                case (ast.expr_unary(_,_,_))    { ret true; }
                case (ast.expr_lit(_,_))        { ret true; }
                case (ast.expr_cast(_,_,_))     { ret true; }
                case (ast.expr_if(_,_,_,_))     { ret false; }
                case (ast.expr_while(_,_,_))    { ret false; }
                case (ast.expr_do_while(_,_,_)) { ret false; }
                case (ast.expr_alt(_,_,_))      { ret false; }
                case (ast.expr_block(_,_))      { ret false; }
                case (ast.expr_assign(_,_,_))   { ret true; }
                case (ast.expr_assign_op(_,_,_,_))
                                                { ret true; }
                case (ast.expr_field(_,_,_))    { ret true; }
                case (ast.expr_index(_,_,_))    { ret true; }
                case (ast.expr_name(_,_,_))     { ret true; }
                case (_)                        { fail; }
            }
        }
        case (_)                                { fail; }
    }
}

impure fn parse_block(parser p) -> ast.block {
    auto lo = p.get_span();

    let vec[@ast.stmt] stmts = vec();
    let option.t[@ast.expr] expr = none[@ast.expr];

    expect(p, token.LBRACE);
    while (p.peek() != token.RBRACE) {
        alt (p.peek()) {
            case (token.RBRACE) {
                // empty; fall through to next iteration
            }
            case (token.SEMI) {
                p.bump();
                // empty
            }
            case (_) {
                auto stmt = parse_stmt(p);
                alt (stmt_to_expr(stmt)) {
                    case (some[@ast.expr](?e)) {
                        alt (p.peek()) {
                            case (token.SEMI) {
                                p.bump();
                                stmts += vec(stmt);
                            }
                            case (token.RBRACE) { expr = some(e); }
                            case (?t) {
                                if (stmt_ends_with_semi(stmt)) {
                                    p.err("expected ';' or '}' after " +
                                          "expression but found " +
                                          token.to_str(t));
                                    fail;
                                }
                                stmts += vec(stmt);
                            }
                        }
                    }
                    case (none[@ast.expr]) {
                        // Not an expression statement.
                        stmts += vec(stmt);
                        if (stmt_ends_with_semi(stmt)) {
                            expect(p, token.SEMI);
                        }
                    }
                }
            }
        }
    }

    p.bump();
    auto hi = p.get_span();

    auto bloc = index_block(stmts, expr);
    ret spanned[ast.block_](lo, hi, bloc);
}

impure fn parse_ty_param(parser p) -> ast.ty_param {
    auto ident = parse_ident(p);
    ret rec(ident=ident, id=p.next_def_id());
}

impure fn parse_ty_params(parser p) -> vec[ast.ty_param] {
    let vec[ast.ty_param] ty_params = vec();
    if (p.peek() == token.LBRACKET) {
        auto f = parse_ty_param;   // FIXME: pass as lval directly
        ty_params = parse_seq[ast.ty_param](token.LBRACKET, token.RBRACKET,
                                            some(token.COMMA), f, p).node;
    }
    ret ty_params;
}

impure fn parse_fn(parser p, ast.effect eff) -> ast._fn {
    auto pf = parse_arg;
    let util.common.spanned[vec[ast.arg]] inputs =
        // FIXME: passing parse_arg as an lval doesn't work at the
        // moment.
        parse_seq[ast.arg]
        (token.LPAREN,
         token.RPAREN,
         some(token.COMMA),
         pf, p);

    let @ast.ty output;
    if (p.peek() == token.RARROW) {
        p.bump();
        output = parse_ty(p);
    } else {
        output = @spanned(inputs.span, inputs.span, ast.ty_nil);
    }

    auto body = parse_block(p);

    ret rec(effect = eff,
            inputs = inputs.node,
            output = output,
            body = body);
}

impure fn parse_item_fn(parser p, ast.effect eff) -> @ast.item {
    auto lo = p.get_span();
    expect(p, token.FN);
    auto id = parse_ident(p);
    auto ty_params = parse_ty_params(p);
    auto f = parse_fn(p, eff);
    auto item = ast.item_fn(id, f, ty_params, p.next_def_id(), ast.ann_none);
    ret @spanned(lo, f.body.span, item);
}


impure fn parse_obj_field(parser p) -> ast.obj_field {
    auto ty = parse_ty(p);
    auto ident = parse_ident(p);
    ret rec(ty=ty, ident=ident, id=p.next_def_id(), ann=ast.ann_none);
}

impure fn parse_method(parser p) -> @ast.method {
    auto lo = p.get_span();
    auto eff = parse_effect(p);
    expect(p, token.FN);
    auto ident = parse_ident(p);
    auto f = parse_fn(p, eff);
    auto meth = rec(ident=ident, meth=f,
                    id=p.next_def_id(), ann=ast.ann_none);
    ret @spanned(lo, f.body.span, meth);
}

impure fn parse_item_obj(parser p, ast.layer lyr) -> @ast.item {
    auto lo = p.get_span();
    expect(p, token.OBJ);
    auto ident = parse_ident(p);
    auto ty_params = parse_ty_params(p);
    auto pf = parse_obj_field;
    let util.common.spanned[vec[ast.obj_field]] fields =
        parse_seq[ast.obj_field]
        (token.LPAREN,
         token.RPAREN,
         some(token.COMMA),
         pf, p);

    auto pm = parse_method;
    let util.common.spanned[vec[@ast.method]] meths =
        parse_seq[@ast.method]
        (token.LBRACE,
         token.RBRACE,
         none[token.token],
         pm, p);

    let ast._obj ob = rec(fields=fields.node,
                          methods=meths.node);

    auto item = ast.item_obj(ident, ob, ty_params,
                             p.next_def_id(), ast.ann_none);

    ret @spanned(lo, meths.span, item);
}

impure fn parse_mod_items(parser p, token.token term) -> ast._mod {
    parse_use_and_imports(p);

    let vec[@ast.item] items = vec();
    auto index = new_str_hash[ast.mod_index_entry]();
    let uint u = 0u;
    while (p.peek() != term) {
        auto item = parse_item(p);
        items += vec(item);

        // Index the item.
        alt (item.node) {
            case (ast.item_const(?id, _, _, _, _)) {
                index.insert(id, ast.mie_item(u));
            }
            case (ast.item_fn(?id, _, _, _, _)) {
                index.insert(id, ast.mie_item(u));
            }
            case (ast.item_mod(?id, _, _)) {
                index.insert(id, ast.mie_item(u));
            }
            case (ast.item_ty(?id, _, _, _, _)) {
                index.insert(id, ast.mie_item(u));
            }
            case (ast.item_tag(?id, ?variants, _, _)) {
                index.insert(id, ast.mie_item(u));
                let uint variant_idx = 0u;
                for (ast.variant v in variants) {
                    index.insert(v.name, ast.mie_tag_variant(u, variant_idx));
                    variant_idx += 1u;
                }
            }
            case (ast.item_obj(?id, _, _, _, _)) {
                index.insert(id, ast.mie_item(u));
            }
        }

        u += 1u;
    }
    ret rec(items=items, index=index);
 }

impure fn parse_item_const(parser p) -> @ast.item {
    auto lo = p.get_span();
    expect(p, token.CONST);
    auto ty = parse_ty(p);
    auto id = parse_ident(p);
    expect(p, token.EQ);
    auto e = parse_expr(p);
    auto hi = p.get_span();
    expect(p, token.SEMI);
    auto item = ast.item_const(id, ty, e, p.next_def_id(), ast.ann_none);
    ret @spanned(lo, hi, item);
}

impure fn parse_item_mod(parser p) -> @ast.item {
    auto lo = p.get_span();
    expect(p, token.MOD);
    auto id = parse_ident(p);
    expect(p, token.LBRACE);
    auto m = parse_mod_items(p, token.RBRACE);
    auto hi = p.get_span();
    expect(p, token.RBRACE);
    auto item = ast.item_mod(id, m, p.next_def_id());
    ret @spanned(lo, hi, item);
}

impure fn parse_item_type(parser p) -> @ast.item {
    auto lo = p.get_span();
    expect(p, token.TYPE);
    auto id = parse_ident(p);
    auto tps = parse_ty_params(p);

    expect(p, token.EQ);
    auto ty = parse_ty(p);
    auto hi = p.get_span();
    expect(p, token.SEMI);
    auto item = ast.item_ty(id, ty, tps, p.next_def_id(), ast.ann_none);
    ret @spanned(lo, hi, item);
}

impure fn parse_item_tag(parser p) -> @ast.item {
    auto lo = p.get_span();
    expect(p, token.TAG);
    auto id = parse_ident(p);
    auto ty_params = parse_ty_params(p);

    let vec[ast.variant] variants = vec();
    expect(p, token.LBRACE);
    while (p.peek() != token.RBRACE) {
        auto tok = p.peek();
        alt (tok) {
            case (token.IDENT(?name)) {
                p.bump();

                let vec[ast.variant_arg] args = vec();
                alt (p.peek()) {
                    case (token.LPAREN) {
                        auto f = parse_ty;
                        auto arg_tys = parse_seq[@ast.ty](token.LPAREN,
                                                          token.RPAREN,
                                                          some(token.COMMA),
                                                          f, p);
                        for (@ast.ty ty in arg_tys.node) {
                            args += vec(rec(ty=ty, id=p.next_def_id()));
                        }
                    }
                    case (_) { /* empty */ }
                }

                expect(p, token.SEMI);

                auto id = p.next_def_id();
                variants += vec(rec(name=name, args=args, id=id,
                                    ann=ast.ann_none));
            }
            case (token.RBRACE) { /* empty */ }
            case (_) {
                p.err("expected name of variant or '}' but found " +
                      token.to_str(tok));
            }
        }
    }
    p.bump();

    auto hi = p.get_span();
    auto item = ast.item_tag(id, variants, ty_params, p.next_def_id());
    ret @spanned(lo, hi, item);
}

impure fn parse_layer(parser p) -> ast.layer {
    alt (p.peek()) {
        case (token.STATE) {
            p.bump();
            ret ast.layer_state;
        }
        case (token.GC) {
            p.bump();
            ret ast.layer_gc;
        }
        case (_) {
            ret ast.layer_value;
        }
    }
    fail;
}


impure fn parse_effect(parser p) -> ast.effect {
    alt (p.peek()) {
        case (token.IMPURE) {
            p.bump();
            ret ast.eff_impure;
        }
        case (token.UNSAFE) {
            p.bump();
            ret ast.eff_unsafe;
        }
        case (_) {
            ret ast.eff_pure;
        }
    }
    fail;
}

fn peeking_at_item(parser p) -> bool {
    alt (p.peek()) {
        case (token.STATE) { ret true; }
        case (token.GC) { ret true; }
        case (token.IMPURE) { ret true; }
        case (token.UNSAFE) { ret true; }
        case (token.CONST) { ret true; }
        case (token.FN) { ret true; }
        case (token.MOD) { ret true; }
        case (token.TYPE) { ret true; }
        case (token.TAG) { ret true; }
        case (token.OBJ) { ret true; }
        case (_) { ret false; }
    }
    ret false;
}

impure fn parse_item(parser p) -> @ast.item {
    let ast.effect eff = parse_effect(p);
    let ast.layer lyr = parse_layer(p);

    alt (p.peek()) {
        case (token.CONST) {
            check (eff == ast.eff_pure);
            check (lyr == ast.layer_value);
            ret parse_item_const(p);
        }

        case (token.FN) {
            check (lyr == ast.layer_value);
            ret parse_item_fn(p, eff);
        }
        case (token.MOD) {
            check (eff == ast.eff_pure);
            check (lyr == ast.layer_value);
            ret parse_item_mod(p);
        }
        case (token.TYPE) {
            check (eff == ast.eff_pure);
            ret parse_item_type(p);
        }
        case (token.TAG) {
            check (eff == ast.eff_pure);
            ret parse_item_tag(p);
        }
        case (token.OBJ) {
            check (eff == ast.eff_pure);
            ret parse_item_obj(p, lyr);
        }
        case (?t) {
            p.err("expected item but found " + token.to_str(t));
        }
    }
    fail;
}

impure fn parse_meta_item(parser p) -> @ast.meta_item {
    auto lo = p.get_span();
    auto hi = lo;
    auto ident = parse_ident(p);
    expect(p, token.EQ);
    alt (p.peek()) {
        case (token.LIT_STR(?s)) {
            p.bump();
            ret @spanned(lo, hi, rec(name = ident, value = s));
        }
        case (_) {
            p.err("Metadata items must be string literals");
        }
    }
    fail;
}

impure fn parse_meta(parser p) -> vec[@ast.meta_item] {
    auto pf = parse_meta_item;
    ret parse_seq[@ast.meta_item](token.LPAREN, token.RPAREN,
                                   some(token.COMMA), pf, p).node;
}

impure fn parse_optional_meta(parser p) -> vec[@ast.meta_item] {
    auto lo = p.get_span();
    auto hi = lo;
    alt (p.peek()) {
        case (token.LPAREN) {
            ret parse_meta(p);
        }
        case (_) {
            let vec[@ast.meta_item] v = vec();
            ret v;
        }
    }
}

impure fn parse_use(parser p) -> @ast.use_node {
    auto lo = p.get_span();
    auto hi = lo;
    expect(p, token.USE);
    auto ident = parse_ident(p);
    auto metadata = parse_optional_meta(p);
    expect(p, token.SEMI);
    ret @spanned(lo, hi, rec(name = ident, metadata = metadata));
}

impure fn parse_rest_import_name(parser p, ast.ident id) -> @ast.import_node {
    auto lo = p.get_span();
    auto hi = lo;
    let vec[ast.ident] identifiers = vec();
    identifiers += id;
    while (p.peek() != token.SEMI) {
        expect(p, token.DOT);
        auto i = parse_ident(p);
        identifiers += i;
    }
    p.bump();
    ret @spanned(lo, hi, rec(identifiers = identifiers));
}

impure fn parse_full_import_name(parser p) -> @ast.import_node {
    alt (p.peek()) {
        case (token.IDENT(?ident)) {
            p.bump();
            ret parse_rest_import_name(p, ident);
        }
        case (_) {
            p.err("expecting an identifier");
        }
    }
    fail;
}

impure fn parse_import(parser p) -> @ast.import_node {
    expect(p, token.IMPORT);
    alt (p.peek()) {
        case (token.IDENT(?ident)) {
            p.bump();
            alt (p.peek()) {
                case (token.EQ) {
                    p.bump();
                    ret parse_full_import_name(p);
                }
                case (_) {
                    ret parse_rest_import_name(p, ident);
                }
            }
        }
        case (_) {
            p.err("expecting an identifier");
        }
    }
    fail;
}

impure fn parse_use_and_imports(parser p) -> vec[ast.use_or_import] {
    let vec[ast.use_or_import] items = vec();
    while (true) {
        alt (p.peek()) {
            case (token.USE) {
                items += vec(ast.use_or_import_use(parse_use(p)));
            }
            case (token.IMPORT) {
                items += vec(ast.use_or_import_import(parse_import(p)));
            }
            case (_) {
                ret items;
            }
        }
    }
}

impure fn parse_crate(parser p) -> @ast.crate {
    auto lo = p.get_span();
    auto hi = lo;
    auto m = parse_mod_items(p, token.EOF);
    ret @spanned(lo, hi, rec(module=m));
}

//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C ../.. 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
//
