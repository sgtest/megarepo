import std.io;
import std._vec;
import std._str;
import std.option;
import std.option.some;
import std.option.none;
import std.map.hashmap;

import driver.session;
import util.common;
import util.common.filename;
import util.common.span;
import util.common.new_str_hash;

tag restriction {
    UNRESTRICTED;
    RESTRICT_NO_CALL_EXPRS;
}

tag file_type {
    CRATE_FILE;
    SOURCE_FILE;
}

state type parser =
    state obj {
          fn peek() -> token.token;
          fn bump();
          fn err(str s);
          fn restrict(restriction r);
          fn get_restriction() -> restriction;
          fn get_file_type() -> file_type;
          fn get_env() -> eval.env;
          fn get_session() -> session.session;
          fn get_span() -> common.span;
          fn get_lo_pos() -> uint;
          fn get_hi_pos() -> uint;
          fn next_def_id() -> ast.def_id;
          fn set_def(ast.def_num);
          fn get_prec_table() -> vec[op_spec];
          fn get_filemap() -> codemap.filemap;
          fn get_chpos() -> uint;
    };

fn new_parser(session.session sess,
                     eval.env env,
                     ast.def_id initial_def,
                     str path, uint pos) -> parser {
    state obj stdio_parser(session.session sess,
                           eval.env env,
                           file_type ftype,
                           mutable token.token tok,
                           mutable uint lo,
                           mutable uint hi,
                           mutable ast.def_num def,
                           mutable restriction res,
                           ast.crate_num crate,
                           lexer.reader rdr,
                           vec[op_spec] precs)
        {
            fn peek() -> token.token {
                ret tok;
            }

            fn bump() {
                // log rdr.get_filename()
                //   + ":" + common.istr(lo.line as int);
                tok = lexer.next_token(rdr);
                lo = rdr.get_mark_chpos();
                hi = rdr.get_chpos();
            }

            fn err(str m) {
                sess.span_err(rec(lo=lo, hi=hi), m);
            }

            fn restrict(restriction r) {
                res = r;
            }

            fn get_restriction() -> restriction {
                ret res;
            }

            fn get_session() -> session.session {
                ret sess;
            }

            fn get_span() -> common.span { ret rec(lo=lo, hi=hi); }
            fn get_lo_pos() -> uint { ret lo; }
            fn get_hi_pos() -> uint { ret hi; }

            fn next_def_id() -> ast.def_id {
                def += 1;
                ret tup(crate, def);
            }

            fn set_def(ast.def_num d) {
                def = d;
            }

            fn get_file_type() -> file_type {
                ret ftype;
            }

            fn get_env() -> eval.env {
                ret env;
            }

            fn get_prec_table() -> vec[op_spec] {
                ret precs;
            }

            fn get_filemap() -> codemap.filemap {
                ret rdr.get_filemap();
            }

            fn get_chpos() -> uint {ret rdr.get_chpos();}
        }
    auto ftype = SOURCE_FILE;
    if (_str.ends_with(path, ".rc")) {
        ftype = CRATE_FILE;
    }
    auto srdr = io.file_reader(path);
    auto filemap = codemap.new_filemap(path, pos);
    _vec.push[codemap.filemap](sess.get_codemap().files, filemap);
    auto rdr = lexer.new_reader(srdr, path, filemap);
    // Make sure npos points at first actual token.
    lexer.consume_any_whitespace(rdr);
    auto npos = rdr.get_chpos();
    ret stdio_parser(sess, env, ftype, lexer.next_token(rdr),
                     npos, npos, initial_def._1, UNRESTRICTED, initial_def._0,
                     rdr, prec_table());
}

fn unexpected(parser p, token.token t) {
    let str s = "unexpected token: ";
    s += token.to_str(t);
    p.err(s);
}

fn expect(parser p, token.token t) {
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

fn spanned[T](uint lo, uint hi, &T node) -> ast.spanned[T] {
    ret rec(node=node, span=rec(lo=lo, hi=hi));
}

fn parse_ident(parser p) -> ast.ident {
    alt (p.peek()) {
        case (token.IDENT(?i)) { p.bump(); ret i; }
        case (_) {
            p.err("expecting ident");
            fail;
        }
    }
}


/* FIXME: gross hack copied from rustboot to make certain configuration-based
 * decisions work at build-time.  We should probably change it to use a
 * lexical sytnax-extension or something similar. For now we just imitate
 * rustboot.
 */
fn parse_str_lit_or_env_ident(parser p) -> ast.ident {
    alt (p.peek()) {
        case (token.LIT_STR(?s)) { p.bump(); ret s; }
        case (token.IDENT(?i)) {
            auto v = eval.lookup(p.get_session(), p.get_env(),
                                 p.get_span(), i);
            if (!eval.val_is_str(v)) {
                p.err("expecting string-valued variable");
            }
            p.bump();
            ret eval.val_as_str(v);
        }
        case (_) {
            p.err("expecting string literal");
            fail;
        }
    }
}


fn parse_ty_fn(ast.proto proto, parser p, uint lo)
    -> ast.ty_ {
    fn parse_fn_input_ty(parser p) -> rec(ast.mode mode, @ast.ty ty) {
        auto mode;
        if (p.peek() == token.BINOP(token.AND)) {
            p.bump();
            mode = ast.alias;

            if (p.peek() == token.MUTABLE) {
                p.bump();
                // TODO: handle mutable alias args
            }
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

    auto lo = p.get_lo_pos();

    auto f = parse_fn_input_ty; // FIXME: trans_const_lval bug
    auto inputs = parse_seq[rec(ast.mode mode, @ast.ty ty)](token.LPAREN,
        token.RPAREN, some(token.COMMA), f, p);

    // FIXME: dropping constrs on the floor at the moment.
    // pick them up when they're used by typestate pass.
    parse_constrs(p);

    let @ast.ty output;
    if (p.peek() == token.RARROW) {
        p.bump();
        output = parse_ty(p);
    } else {
        output = @spanned(lo, inputs.span.hi, ast.ty_nil);
    }

    ret ast.ty_fn(proto, inputs.node, output);
}

fn parse_proto(parser p) -> ast.proto {
    alt (p.peek()) {
        case (token.ITER) { p.bump(); ret ast.proto_iter; }
        case (token.FN) { p.bump(); ret ast.proto_fn; }
        case (?t) { unexpected(p, t); }
    }
    fail;
}

fn parse_ty_obj(parser p, &mutable uint hi) -> ast.ty_ {
    expect(p, token.OBJ);
    fn parse_method_sig(parser p) -> ast.ty_method {
        auto flo = p.get_lo_pos();

        let ast.proto proto = parse_proto(p);
        auto ident = parse_ident(p);
        auto f = parse_ty_fn(proto, p, flo);
        expect(p, token.SEMI);
        alt (f) {
            case (ast.ty_fn(?proto, ?inputs, ?output)) {
                ret rec(proto=proto, ident=ident,
                        inputs=inputs, output=output);
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
    hi = meths.span.hi;
    ret ast.ty_obj(meths.node);
}

fn parse_mt(parser p) -> ast.mt {
    auto mut = parse_mutability(p);
    auto t = parse_ty(p);
    ret rec(ty=t, mut=mut);
}

fn parse_ty_field(parser p) -> ast.ty_field {
    auto mt = parse_mt(p);
    auto id = parse_ident(p);
    ret rec(ident=id, mt=mt);
}

fn parse_constr_arg(parser p) -> @ast.constr_arg {
    auto sp = p.get_span();
    auto carg = ast.carg_base;
    if (p.peek() == token.BINOP(token.STAR)) {
        p.bump();
    } else {
        carg = ast.carg_ident(parse_ident(p));
    }
    ret @rec(node=carg, span=sp);
}

fn parse_ty_constr(parser p) -> @ast.constr {
    auto lo = p.get_lo_pos();
    auto path = parse_path(p, GREEDY);
    auto pf = parse_constr_arg;
    auto args = parse_seq[@ast.constr_arg](token.LPAREN,
                                         token.RPAREN,
                                         some(token.COMMA), pf, p);
    ret @spanned(lo, args.span.hi, rec(path=path, args=args.node));
}

fn parse_constrs(parser p) -> common.spanned[vec[@ast.constr]] {
    auto lo = p.get_lo_pos();
    auto hi = p.get_hi_pos();
    let vec[@ast.constr] constrs = vec();
    if (p.peek() == token.COLON) {
        p.bump();
        let bool more = true;
        while (more) {
            alt (p.peek()) {
                case (token.IDENT(_)) {
                    auto constr = parse_ty_constr(p);
                    hi = constr.span.hi;
                    _vec.push[@ast.constr](constrs, constr);
                    if (p.peek() == token.COMMA) {
                        p.bump();
                        more = false;
                    }
                }
                case (_) { more = false; }
            }
        }
    }
   ret spanned(lo, hi, constrs);
}

fn parse_ty_constrs(@ast.ty t, parser p) -> @ast.ty {
   if (p.peek() == token.COLON) {
       auto constrs = parse_constrs(p);
       ret @spanned(t.span.lo, constrs.span.hi,
                    ast.ty_constr(t, constrs.node));
   }
   ret t;
}

fn parse_ty(parser p) -> @ast.ty {
    auto lo = p.get_lo_pos();
    auto hi = lo;
    let ast.ty_ t;

    // FIXME: do something with this
    let ast.layer lyr = parse_layer(p);

    alt (p.peek()) {
        case (token.BOOL) { p.bump(); t = ast.ty_bool; }
        case (token.INT) { p.bump(); t = ast.ty_int; }
        case (token.UINT) { p.bump(); t = ast.ty_uint; }
        case (token.FLOAT) { p.bump(); t = ast.ty_float; }
        case (token.STR) { p.bump(); t = ast.ty_str; }
        case (token.CHAR) { p.bump(); t = ast.ty_char; }
        case (token.MACH(?tm)) { p.bump(); t = ast.ty_machine(tm); }

        case (token.LPAREN) {
            p.bump();
            alt (p.peek()) {
                case (token.RPAREN) {
                    hi = p.get_hi_pos();
                    p.bump();
                    t = ast.ty_nil;
                }
                case (_) {
                    t = parse_ty(p).node;
                    hi = p.get_hi_pos();
                    expect(p, token.RPAREN);
                }
            }
        }

        case (token.AT) {
            p.bump();
            auto mt = parse_mt(p);
            hi = mt.ty.span.hi;
            t = ast.ty_box(mt);
        }

        case (token.VEC) {
            p.bump();
            expect(p, token.LBRACKET);
            t = ast.ty_vec(parse_mt(p));
            hi = p.get_hi_pos();
            expect(p, token.RBRACKET);
        }

        case (token.TUP) {
            p.bump();
            auto f = parse_mt; // FIXME: trans_const_lval bug
            auto elems = parse_seq[ast.mt] (token.LPAREN,
                                            token.RPAREN,
                                            some(token.COMMA), f, p);
            hi = elems.span.hi;
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
            hi = elems.span.hi;
            t = ast.ty_rec(elems.node);
        }

        case (token.FN) {
            auto flo = p.get_lo_pos();
            p.bump();
            t = parse_ty_fn(ast.proto_fn, p, flo);
            alt (t) {
                case (ast.ty_fn(_, _, ?out)) {
                    hi = out.span.hi;
                }
            }
        }

        case (token.ITER) {
            auto flo = p.get_lo_pos();
            p.bump();
            t = parse_ty_fn(ast.proto_iter, p, flo);
            alt (t) {
                case (ast.ty_fn(_, _, ?out)) {
                    hi = out.span.hi;
                }
            }
        }

        case (token.OBJ) {
            t = parse_ty_obj(p, hi);
        }

        case (token.PORT) {
            p.bump();
            expect(p, token.LBRACKET);
            t = ast.ty_port(parse_ty(p));
            hi = p.get_hi_pos();
            expect(p, token.RBRACKET);
        }

        case (token.CHAN) {
            p.bump();
            expect(p, token.LBRACKET);
            t = ast.ty_chan(parse_ty(p));
            hi = p.get_hi_pos();
            expect(p, token.RBRACKET);
        }

        case (token.IDENT(_)) {
            auto path = parse_path(p, GREEDY);
            t = ast.ty_path(path, none[ast.def]);
            hi = path.span.hi;
        }

        case (token.MUTABLE) {
            p.bump();
            p.get_session().span_warn(p.get_span(),
                "ignoring deprecated 'mutable' type constructor");
            auto typ = parse_ty(p);
            t = typ.node;
            hi = typ.span.hi;
        }

        case (_) {
            p.err("expecting type");
            t = ast.ty_nil;
            fail;
        }
    }

    ret parse_ty_constrs(@spanned(lo, hi, t), p);
}

fn parse_arg(parser p) -> ast.arg {
    let ast.mode m = ast.val;
    if (p.peek() == token.BINOP(token.AND)) {
        m = ast.alias;
        p.bump();

        if (p.peek() == token.MUTABLE) {
            // TODO: handle mutable alias args
            p.bump();
        }
    }
    let @ast.ty t = parse_ty(p);
    let ast.ident i = parse_ident(p);
    ret rec(mode=m, ty=t, ident=i, id=p.next_def_id());
}

fn parse_seq_to_end[T](token.token ket,
                              option.t[token.token] sep,
                              (fn(parser) -> T) f,
                              mutable uint hi,
                              parser p) -> vec[T] {
    let bool first = true;
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
    hi = p.get_hi_pos();
    expect(p, ket);
    ret v;
}

fn parse_seq[T](token.token bra,
                       token.token ket,
                       option.t[token.token] sep,
                       (fn(parser) -> T) f,
                       parser p) -> util.common.spanned[vec[T]] {
    auto lo = p.get_lo_pos();
    auto hi = p.get_hi_pos();
    expect(p, bra);
    auto result = parse_seq_to_end[T](ket, sep, f, hi, p);
    ret spanned(lo, hi, result);
}

fn parse_lit(parser p) -> ast.lit {
    auto sp = p.get_span();
    let ast.lit_ lit = ast.lit_nil;
    alt (p.peek()) {
        case (token.LIT_INT(?i)) {
            p.bump();
            lit = ast.lit_int(i);
        }
        case (token.LIT_UINT(?u)) {
            p.bump();
            lit = ast.lit_uint(u);
        }
        case (token.LIT_FLOAT(?s)) {
            p.bump();
            lit = ast.lit_float(s);
        }
        case (token.LIT_MACH_INT(?tm, ?i)) {
            p.bump();
            lit = ast.lit_mach_int(tm, i);
        }
        case (token.LIT_MACH_FLOAT(?tm, ?s)) {
            p.bump();
            lit = ast.lit_mach_float(tm, s);
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
        case (?t) {
            unexpected(p, t);
        }
    }
    ret rec(node=lit, span=sp);
}

fn is_ident(token.token t) -> bool {
    alt (t) {
        case (token.IDENT(_)) { ret true; }
        case (_) {}
    }
    ret false;
}

tag greed {
    GREEDY;
    MINIMAL;
}

fn parse_ty_args(parser p, uint hi) ->
    util.common.spanned[vec[@ast.ty]] {

    if (p.peek() == token.LBRACKET) {
        auto pf = parse_ty;
        ret parse_seq[@ast.ty](token.LBRACKET,
                               token.RBRACKET,
                               some(token.COMMA),
                               pf, p);
    }
    let vec[@ast.ty] v = vec();
    auto pos = p.get_lo_pos();
    ret spanned(hi, hi, v);
}

fn parse_path(parser p, greed g) -> ast.path {

    auto lo = p.get_lo_pos();
    auto hi = lo;

    let vec[ast.ident] ids = vec();
    let bool more = true;
    while (more) {
        alt (p.peek()) {
            case (token.IDENT(?i)) {
                hi = p.get_hi_pos();
                ids += vec(i);
                p.bump();
                if (p.peek() == token.DOT) {
                    if (g == GREEDY) {
                        p.bump();
                        check (is_ident(p.peek()));
                    } else {
                        more = false;
                    }
                } else {
                    more = false;
                }
            }
            case (_) {
                more = false;
            }
        }
    }

    auto tys = parse_ty_args(p, hi);
    ret spanned(lo, tys.span.hi, rec(idents=ids, types=tys.node));
}

fn parse_mutability(parser p) -> ast.mutability {
    if (p.peek() == token.MUTABLE) {
        p.bump();
        if (p.peek() == token.QUES) {
            p.bump();
            ret ast.maybe_mut;
        }
        ret ast.mut;
    }
    ret ast.imm;
}

fn parse_field(parser p) -> ast.field {
    auto m = parse_mutability(p);
    auto i = parse_ident(p);
    expect(p, token.EQ);
    auto e = parse_expr(p);
    ret rec(mut=m, ident=i, expr=e);
}

fn parse_bottom_expr(parser p) -> @ast.expr {

    auto lo = p.get_lo_pos();
    auto hi = p.get_hi_pos();

    // FIXME: can only remove this sort of thing when both typestate and
    // alt-exhaustive-match checking are co-operating.
    auto lit = @spanned(lo, hi, ast.lit_nil);
    let ast.expr_ ex = ast.expr_lit(lit, ast.ann_none);

    alt (p.peek()) {

        case (token.IDENT(_)) {
            auto pth = parse_path(p, MINIMAL);
            hi = pth.span.hi;
            ex = ast.expr_path(pth, none[ast.def], ast.ann_none);
        }

        case (token.LPAREN) {
            p.bump();
            alt (p.peek()) {
                case (token.RPAREN) {
                    hi = p.get_hi_pos();
                    p.bump();
                    auto lit = @spanned(lo, hi, ast.lit_nil);
                    ret @spanned(lo, hi,
                                 ast.expr_lit(lit, ast.ann_none));
                }
                case (_) { /* fall through */ }
            }
            auto e = parse_expr(p);
            hi = p.get_hi_pos();
            expect(p, token.RPAREN);
            ret @spanned(lo, hi, e.node);
        }

        case (token.TUP) {
            p.bump();
            fn parse_elt(parser p) -> ast.elt {
                auto m = parse_mutability(p);
                auto e = parse_expr(p);
                ret rec(mut=m, expr=e);
            }
            auto pf = parse_elt;
            auto es =
                parse_seq[ast.elt](token.LPAREN,
                                   token.RPAREN,
                                   some(token.COMMA),
                                   pf, p);
            hi = es.span.hi;
            ex = ast.expr_tup(es.node, ast.ann_none);
        }

        case (token.VEC) {
            p.bump();
            auto pf = parse_expr;

            expect(p, token.LPAREN);
            auto mut = parse_mutability(p);

            auto es = parse_seq_to_end[@ast.expr](token.RPAREN,
                                                  some(token.COMMA),
                                                  pf, hi, p);
            ex = ast.expr_vec(es, mut, ast.ann_none);
        }

        case (token.REC) {
            p.bump();
            expect(p, token.LPAREN);
            auto fields = vec(parse_field(p));

            auto more = true;
            auto base = none[@ast.expr];
            while (more) {
                alt (p.peek()) {
                    case (token.RPAREN) {
                        hi = p.get_hi_pos();
                        p.bump();
                        more = false;
                    }
                    case (token.WITH) {
                        p.bump();
                        base = some[@ast.expr](parse_expr(p));
                        hi = p.get_hi_pos();
                        expect(p, token.RPAREN);
                        more = false;
                    }
                    case (token.COMMA) {
                        p.bump();
                        fields += vec(parse_field(p));
                    }
                    case (?t) {
                        unexpected(p, t);
                    }
                }

            }

            ex = ast.expr_rec(fields, base, ast.ann_none);
        }

        case (token.BIND) {
            p.bump();
            auto e = parse_expr_res(p, RESTRICT_NO_CALL_EXPRS);
            fn parse_expr_opt(parser p) -> option.t[@ast.expr] {
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
            hi = es.span.hi;
            ex = ast.expr_bind(e, es.node, ast.ann_none);
        }

        case (token.POUND) {
            p.bump();
            auto pth = parse_path(p, GREEDY);
            auto pf = parse_expr;
            auto es = parse_seq[@ast.expr](token.LPAREN,
                                           token.RPAREN,
                                           some(token.COMMA),
                                           pf, p);
            hi = es.span.hi;
            ex = expand_syntax_ext(p, es.span, pth, es.node,
                                   none[str]);
        }

        case (token.FAIL) {
            p.bump();
            ex = ast.expr_fail(ast.ann_none);
        }

        case (token.LOG) {
            p.bump();
            auto e = parse_expr(p);
            auto hi = e.span.hi;
            ex = ast.expr_log(1, e, ast.ann_none);
        }

        case (token.LOG_ERR) {
            p.bump();
            auto e = parse_expr(p);
            auto hi = e.span.hi;
            ex = ast.expr_log(0, e, ast.ann_none);
        }

        case (token.CHECK) {
            p.bump();
            alt (p.peek()) {
                case (token.LPAREN) {
                    auto e = parse_expr(p);
                    auto hi = e.span.hi;
                    ex = ast.expr_check_expr(e, ast.ann_none);
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
                    ex = ast.expr_ret(none[@ast.expr], ast.ann_none);
                }
                case (_) {
                    auto e = parse_expr(p);
                    hi = e.span.hi;
                    ex = ast.expr_ret(some[@ast.expr](e), ast.ann_none);
                }
            }
        }

        case (token.BREAK) {
            p.bump();
            ex = ast.expr_break(ast.ann_none);
        }

        case (token.CONT) {
            p.bump();
            ex = ast.expr_cont(ast.ann_none);
        }

        case (token.PUT) {
            p.bump();
            alt (p.peek()) {
                case (token.SEMI) {
                    ex = ast.expr_put(none[@ast.expr], ast.ann_none);
                }
                case (_) {
                    auto e = parse_expr(p);
                    hi = e.span.hi;
                    ex = ast.expr_put(some[@ast.expr](e), ast.ann_none);
                }
            }
        }

        case (token.BE) {
            p.bump();
            auto e = parse_expr(p);
            // FIXME: Is this the right place for this check?
            if /*check*/ (ast.is_call_expr(e)) {
                    hi = e.span.hi;
                    ex = ast.expr_be(e, ast.ann_none);
            }
            else {
                p.err("Non-call expression in tail call");
            }
        }

        case (token.PORT) {
            p.bump();
            expect(p, token.LPAREN);
            expect(p, token.RPAREN);
            hi = p.get_hi_pos();
            ex = ast.expr_port(ast.ann_none);
        }

        case (token.CHAN) {
            p.bump();
            expect(p, token.LPAREN);
            auto e = parse_expr(p);
            hi = e.span.hi;
            expect(p, token.RPAREN);
            ex = ast.expr_chan(e, ast.ann_none);
        }

        case (token.SELF) {
            log "parsing a self-call...";

            p.bump();
            expect(p, token.DOT);
            // The rest is a call expression.
            let @ast.expr f = parse_self_method(p);
            auto pf = parse_expr;
            auto es = parse_seq[@ast.expr](token.LPAREN,
                                           token.RPAREN,
                                           some(token.COMMA),
                                           pf, p);
            hi = es.span.hi;
            ex = ast.expr_call(f, es.node, ast.ann_none);
        }

        case (_) {
            auto lit = parse_lit(p);
            hi = lit.span.hi;
            ex = ast.expr_lit(@lit, ast.ann_none);
        }
    }

    ret @spanned(lo, hi, ex);
}

/*
 * FIXME: This is a crude approximation of the syntax-extension system,
 * for purposes of prototyping and/or hard-wiring any extensions we
 * wish to use while bootstrapping. The eventual aim is to permit
 * loading rust crates to process extensions, but this will likely
 * require a rust-based frontend, or an ocaml-FFI-based connection to
 * rust crates. At the moment we have neither.
 */

fn expand_syntax_ext(parser p, ast.span sp,
                     &ast.path path, vec[@ast.expr] args,
                     option.t[str] body) -> ast.expr_ {

    check (_vec.len[ast.ident](path.node.idents) > 0u);
    auto extname = path.node.idents.(0);
    if (_str.eq(extname, "fmt")) {
        auto expanded = extfmt.expand_syntax_ext(args, body);
        auto newexpr = ast.expr_ext(path, args, body,
                                    expanded,
                                    ast.ann_none);

        ret newexpr;
    } else {
        p.err("unknown syntax extension");
        fail;
    }
}

fn extend_expr_by_ident(parser p, uint lo, uint hi,
                               @ast.expr e, ast.ident i) -> @ast.expr {
    auto e_ = e.node;
    alt (e.node) {
        case (ast.expr_path(?pth, ?def, ?ann)) {
            if (_vec.len[@ast.ty](pth.node.types) == 0u) {
                auto idents_ = pth.node.idents;
                idents_ += vec(i);
                auto tys = parse_ty_args(p, hi);
                auto pth_ = spanned(pth.span.lo, tys.span.hi,
                                    rec(idents=idents_,
                                        types=tys.node));
                e_ = ast.expr_path(pth_, def, ann);
                ret @spanned(pth_.span.lo, pth_.span.hi, e_);
            } else {
                e_ = ast.expr_field(e, i, ann);
            }
        }
        case (_) {
            e_ = ast.expr_field(e, i, ast.ann_none);
        }
    }
    ret @spanned(lo, hi, e_);
}

fn parse_self_method(parser p) -> @ast.expr {
    auto sp = p.get_span();
    let ast.ident f_name = parse_ident(p);
    auto hi = p.get_span();
    ret @rec(node=ast.expr_self_method(f_name, ast.ann_none), span=sp);
}

fn parse_dot_or_call_expr(parser p) -> @ast.expr {
    auto lo = p.get_lo_pos();
    auto e = parse_bottom_expr(p);
    auto hi = e.span.hi;
    while (true) {
        alt (p.peek()) {

            case (token.LPAREN) {
                if (p.get_restriction() == RESTRICT_NO_CALL_EXPRS) {
                    ret e;
                } else {
                    // Call expr.
                    auto pf = parse_expr;
                    auto es = parse_seq[@ast.expr](token.LPAREN,
                                                   token.RPAREN,
                                                   some(token.COMMA),
                                                   pf, p);
                    hi = es.span.hi;
                    auto e_ = ast.expr_call(e, es.node, ast.ann_none);
                    e = @spanned(lo, hi, e_);
                }
            }

            case (token.DOT) {
                p.bump();
                alt (p.peek()) {

                    case (token.IDENT(?i)) {
                        hi = p.get_hi_pos();
                        p.bump();
                        e = extend_expr_by_ident(p, lo, hi, e, i);
                    }

                    case (token.LPAREN) {
                        p.bump();
                        auto ix = parse_expr(p);
                        hi = ix.span.hi;
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

fn parse_prefix_expr(parser p) -> @ast.expr {

    if (p.peek() == token.MUTABLE) {
        p.bump();
        p.get_session().span_warn(p.get_span(),
            "ignoring deprecated 'mutable' prefix operator");
    }

    auto lo = p.get_lo_pos();
    auto hi = p.get_hi_pos();

    // FIXME: can only remove this sort of thing when both typestate and
    // alt-exhaustive-match checking are co-operating.
    auto lit = @spanned(lo, lo, ast.lit_nil);
    let ast.expr_ ex = ast.expr_lit(lit, ast.ann_none);

    alt (p.peek()) {

        case (token.NOT) {
            p.bump();
            auto e = parse_prefix_expr(p);
            hi = e.span.hi;
            ex = ast.expr_unary(ast.not, e, ast.ann_none);
        }

        case (token.TILDE) {
            p.bump();
            auto e = parse_prefix_expr(p);
            hi = e.span.hi;
            ex = ast.expr_unary(ast.bitnot, e, ast.ann_none);
        }

        case (token.BINOP(?b)) {
            alt (b) {
                case (token.MINUS) {
                    p.bump();
                    auto e = parse_prefix_expr(p);
                    hi = e.span.hi;
                    ex = ast.expr_unary(ast.neg, e, ast.ann_none);
                }

                case (token.STAR) {
                    p.bump();
                    auto e = parse_prefix_expr(p);
                    hi = e.span.hi;
                    ex = ast.expr_unary(ast.deref, e, ast.ann_none);
                }

                case (_) {
                    ret parse_dot_or_call_expr(p);
                }
            }
        }

        case (token.AT) {
            p.bump();
            auto m = parse_mutability(p);
            auto e = parse_prefix_expr(p);
            hi = e.span.hi;
            ex = ast.expr_unary(ast.box(m), e, ast.ann_none);
        }

        case (_) {
            ret parse_dot_or_call_expr(p);
        }
    }
    ret @spanned(lo, hi, ex);
}

type op_spec = rec(token.token tok, ast.binop op, int prec);

// FIXME make this a const, don't store it in parser state
fn prec_table() -> vec[op_spec] {
    ret vec(rec(tok=token.BINOP(token.STAR), op=ast.mul, prec=11),
            rec(tok=token.BINOP(token.SLASH), op=ast.div, prec=11),
            rec(tok=token.BINOP(token.PERCENT), op=ast.rem, prec=11),
            rec(tok=token.BINOP(token.PLUS), op=ast.add, prec=10),
            rec(tok=token.BINOP(token.MINUS), op=ast.sub, prec=10),
            rec(tok=token.BINOP(token.LSL), op=ast.lsl, prec=9),
            rec(tok=token.BINOP(token.LSR), op=ast.lsr, prec=9),
            rec(tok=token.BINOP(token.ASR), op=ast.asr, prec=9),
            rec(tok=token.BINOP(token.AND), op=ast.bitand, prec=8),
            rec(tok=token.BINOP(token.CARET), op=ast.bitxor, prec=6),
            rec(tok=token.BINOP(token.OR), op=ast.bitor, prec=6),
            // ast.mul is a bogus placeholder here, AS is special
            // cased in parse_more_binops
            rec(tok=token.AS, op=ast.mul, prec=5),
            rec(tok=token.LT, op=ast.lt, prec=4),
            rec(tok=token.LE, op=ast.le, prec=4),
            rec(tok=token.GE, op=ast.ge, prec=4),
            rec(tok=token.GT, op=ast.gt, prec=4),
            rec(tok=token.EQEQ, op=ast.eq, prec=3),
            rec(tok=token.NE, op=ast.ne, prec=3),
            rec(tok=token.ANDAND, op=ast.and, prec=2),
            rec(tok=token.OROR, op=ast.or, prec=1));
}

fn parse_binops(parser p) -> @ast.expr {
    ret parse_more_binops(p, parse_prefix_expr(p), 0);
}

fn parse_more_binops(parser p, @ast.expr lhs, int min_prec)
    -> @ast.expr {
    // Magic nonsense to work around rustboot bug
    fn op_eq(token.token a, token.token b) -> bool {
        if (a == b) {ret true;}
        else {ret false;}
    }
    auto peeked = p.peek();
    for (op_spec cur in p.get_prec_table()) {
        if (cur.prec > min_prec && op_eq(cur.tok, peeked)) {
            p.bump();
            alt (cur.tok) {
                case (token.AS) {
                    auto rhs = parse_ty(p);
                    auto _as = ast.expr_cast(lhs, rhs, ast.ann_none);
                    auto span = @spanned(lhs.span.lo, rhs.span.hi, _as);
                    ret parse_more_binops(p, span, min_prec);
                }
                case (_) {
                    auto rhs = parse_more_binops(p, parse_prefix_expr(p),
                                                 cur.prec);
                    auto bin = ast.expr_binary(cur.op, lhs, rhs,
                                               ast.ann_none);
                    auto span = @spanned(lhs.span.lo, rhs.span.hi, bin);
                    ret parse_more_binops(p, span, min_prec);
                }
            }
        }
    }
    ret lhs;
}

fn parse_assign_expr(parser p) -> @ast.expr {
    auto lo = p.get_lo_pos();
    auto lhs = parse_binops(p);
    alt (p.peek()) {
        case (token.EQ) {
            p.bump();
            auto rhs = parse_expr(p);
            ret @spanned(lo, rhs.span.hi,
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
            ret @spanned(lo, rhs.span.hi,
                         ast.expr_assign_op(aop, lhs, rhs, ast.ann_none));
        }
        case (token.SEND) {
            p.bump();
            auto rhs = parse_expr(p);
            ret @spanned(lo, rhs.span.hi,
                         ast.expr_send(lhs, rhs, ast.ann_none));
        }
        case (token.LARROW) {
            p.bump();
            auto rhs = parse_expr(p);
            ret @spanned(lo, rhs.span.hi,
                         ast.expr_recv(lhs, rhs, ast.ann_none));
        }
        case (_) { /* fall through */ }
    }
    ret lhs;
}

fn parse_if_expr(parser p) -> @ast.expr {
    auto lo = p.get_lo_pos();

    expect(p, token.IF);
    expect(p, token.LPAREN);
    auto cond = parse_expr(p);
    expect(p, token.RPAREN);
    auto thn = parse_block(p);
    let option.t[@ast.expr] els = none[@ast.expr];
    auto hi = thn.span.hi;
    alt (p.peek()) {
        case (token.ELSE) {
            auto elexpr = parse_else_expr(p);
            els = some(elexpr);
            hi = elexpr.span.hi;
        }
        case (_) { /* fall through */ }
    }

    ret @spanned(lo, hi, ast.expr_if(cond, thn, els, ast.ann_none));
}

fn parse_else_expr(parser p) -> @ast.expr {
    expect(p, token.ELSE);
    alt (p.peek()) {
        case (token.IF) {
            ret parse_if_expr(p);
        }
        case (_) {
            auto blk = parse_block(p);
            ret @spanned(blk.span.lo, blk.span.hi,
                         ast.expr_block(blk, ast.ann_none));
        }
    }
}

fn parse_head_local(parser p) -> @ast.decl {
    auto lo = p.get_lo_pos();
    let @ast.local local;
    if (p.peek() == token.AUTO) {
        local = parse_auto_local(p);
    } else {
        local = parse_typed_local(p);
    }
    ret @spanned(lo, p.get_hi_pos(), ast.decl_local(local));
}



fn parse_for_expr(parser p) -> @ast.expr {
    auto lo = p.get_lo_pos();
    auto is_each = false;

    expect(p, token.FOR);
    if (p.peek() == token.EACH) {
        is_each = true;
        p.bump();
    }

    expect (p, token.LPAREN);

    auto decl = parse_head_local(p);
    expect(p, token.IN);

    auto seq = parse_expr(p);
    expect(p, token.RPAREN);
    auto body = parse_block(p);
    auto hi = body.span.hi;
    if (is_each) {
        ret @spanned(lo, hi, ast.expr_for_each(decl, seq, body,
                                                ast.ann_none));
    } else {
        ret @spanned(lo, hi, ast.expr_for(decl, seq, body,
                                          ast.ann_none));
    }
}


fn parse_while_expr(parser p) -> @ast.expr {
    auto lo = p.get_lo_pos();

    expect(p, token.WHILE);
    expect (p, token.LPAREN);
    auto cond = parse_expr(p);
    expect(p, token.RPAREN);
    auto body = parse_block(p);
    auto hi = body.span.hi;
    ret @spanned(lo, hi, ast.expr_while(cond, body, ast.ann_none));
}

fn parse_do_while_expr(parser p) -> @ast.expr {
    auto lo = p.get_lo_pos();

    expect(p, token.DO);
    auto body = parse_block(p);
    expect(p, token.WHILE);
    expect (p, token.LPAREN);
    auto cond = parse_expr(p);
    expect(p, token.RPAREN);
    auto hi = cond.span.hi;
    ret @spanned(lo, hi, ast.expr_do_while(body, cond, ast.ann_none));
}

fn parse_alt_expr(parser p) -> @ast.expr {
    auto lo = p.get_lo_pos();
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

            // FIXME: this is a vestigial form left over from
            // rustboot, we're keeping it here for source-compat
            // for the time being but it should be flushed out
            // once we've bootstrapped. When we see 'else {' here,
            // we pretend we saw 'case (_) {'. It has the same
            // meaning, and only exists due to the cexp/pexp split
            // in rustboot, which we're not maintaining.

            case (token.ELSE) {
                p.bump();
                auto hi = p.get_hi_pos();
                auto pat = @spanned(lo, hi, ast.pat_wild(ast.ann_none));
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
    auto hi = p.get_hi_pos();
    p.bump();

    auto expr = ast.expr_alt(discriminant, arms, ast.ann_none);
    ret @spanned(lo, hi, expr);
}

fn parse_spawn_expr(parser p) -> @ast.expr {
    auto lo = p.get_lo_pos();
    expect(p, token.SPAWN);

    // FIXME: Parse domain and name

    auto fn_expr = parse_bottom_expr(p);
    auto pf = parse_expr;
    auto es = parse_seq[@ast.expr](token.LPAREN,
                                   token.RPAREN,
                                   some(token.COMMA),
                                   pf, p);
    auto hi = es.span.hi;
    auto spawn_expr = ast.expr_spawn(ast.dom_implicit,
                                     option.none[str],
                                     fn_expr,
                                     es.node,
                                     ast.ann_none);
    ret @spanned(lo, hi, spawn_expr);
}

fn parse_expr(parser p) -> @ast.expr {
    ret parse_expr_res(p, UNRESTRICTED);
}

fn parse_expr_res(parser p, restriction r) -> @ast.expr {
    auto old = p.get_restriction();
    p.restrict(r);
    auto e = parse_expr_inner(p);
    p.restrict(old);
    ret e;
}

fn parse_expr_inner(parser p) -> @ast.expr {
    alt (p.peek()) {
        case (token.LBRACE) {
            auto blk = parse_block(p);
            ret @spanned(blk.span.lo, blk.span.hi,
                         ast.expr_block(blk, ast.ann_none));
        }
        case (token.IF) {
            ret parse_if_expr(p);
        }
        case (token.FOR) {
            ret parse_for_expr(p);
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
        case (token.SPAWN) {
            ret parse_spawn_expr(p);
        }
        case (_) {
            ret parse_assign_expr(p);
        }

    }
}

fn parse_initializer(parser p) -> option.t[ast.initializer] {
    alt (p.peek()) {
        case (token.EQ) {
            p.bump();
            ret some(rec(op = ast.init_assign,
                         expr = parse_expr(p)));
        }
        case (token.LARROW) {
            p.bump();
            ret some(rec(op = ast.init_recv,
                         expr = parse_expr(p)));
        }
        case (_) {
            ret none[ast.initializer];
        }
    }
}

fn parse_pat(parser p) -> @ast.pat {
    auto lo = p.get_lo_pos();
    auto hi = p.get_hi_pos();
    auto pat;

    alt (p.peek()) {
        case (token.UNDERSCORE) {
            p.bump();
            pat = ast.pat_wild(ast.ann_none);
        }
        case (token.QUES) {
            p.bump();
            alt (p.peek()) {
                case (token.IDENT(?id)) {
                    hi = p.get_hi_pos();
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
            auto tag_path = parse_path(p, GREEDY);
            hi = tag_path.span.hi;

            let vec[@ast.pat] args;
            alt (p.peek()) {
                case (token.LPAREN) {
                    auto f = parse_pat;
                    auto a = parse_seq[@ast.pat](token.LPAREN, token.RPAREN,
                                                 some(token.COMMA), f, p);
                    args = a.node;
                    hi = a.span.hi;
                }
                case (_) { args = vec(); }
            }

            pat = ast.pat_tag(tag_path, args, none[ast.variant_def],
                              ast.ann_none);
        }
        case (_) {
            auto lit = parse_lit(p);
            hi = lit.span.hi;
            pat = ast.pat_lit(@lit, ast.ann_none);
        }
    }

    ret @spanned(lo, hi, pat);
}

fn parse_local_full(&option.t[@ast.ty] tyopt,
                           parser p) -> @ast.local {
    auto ident = parse_ident(p);
    auto init = parse_initializer(p);
    ret @rec(ty = tyopt,
             infer = false,
             ident = ident,
             init = init,
             id = p.next_def_id(),
             ann = ast.ann_none);
}

fn parse_typed_local(parser p) -> @ast.local {
    auto ty = parse_ty(p);
    ret parse_local_full(some(ty), p);
}

fn parse_auto_local(parser p) -> @ast.local {
    ret parse_local_full(none[@ast.ty], p);
}

fn parse_let(parser p) -> @ast.decl {
    auto lo = p.get_lo_pos();
    expect(p, token.LET);
    auto local = parse_typed_local(p);
    ret @spanned(lo, p.get_hi_pos(), ast.decl_local(local));
}

fn parse_auto(parser p) -> @ast.decl {
    auto lo = p.get_lo_pos();
    expect(p, token.AUTO);
    auto local = parse_auto_local(p);
    ret @spanned(lo, p.get_hi_pos(), ast.decl_local(local));
}

fn parse_stmt(parser p) -> @ast.stmt {
    if (p.get_file_type() == SOURCE_FILE) {
        ret parse_source_stmt(p);
    } else {
        ret parse_crate_stmt(p);
    }
}

fn parse_crate_stmt(parser p) -> @ast.stmt {
    auto cdir = parse_crate_directive(p);
    ret @spanned(cdir.span.lo, cdir.span.hi,
                 ast.stmt_crate_directive(@cdir));
}

fn parse_source_stmt(parser p) -> @ast.stmt {
    auto lo = p.get_lo_pos();
    alt (p.peek()) {

        case (token.LET) {
            auto decl = parse_let(p);
            auto hi = p.get_span();
            ret @spanned
                (lo, decl.span.hi, ast.stmt_decl(decl, ast.ann_none));
        }

        case (token.AUTO) {
            auto decl = parse_auto(p);
            auto hi = p.get_span();
            ret @spanned(lo, decl.span.hi, ast.stmt_decl(decl, ast.ann_none));
        }

        case (_) {
            if (peeking_at_item(p)) {
                // Might be a local item decl.
                auto i = parse_item(p);
                auto hi = i.span.hi;
                auto decl = @spanned(lo, hi, ast.decl_item(i));
                ret @spanned(lo, hi, ast.stmt_decl(decl, ast.ann_none));

            } else {
                // Remainder are line-expr stmts.
                auto e = parse_expr(p);
                auto hi = p.get_span();
                ret @spanned(lo, e.span.hi, ast.stmt_expr(e, ast.ann_none));
            }
        }
    }
    p.err("expected statement");
    fail;
}

fn index_block(vec[@ast.stmt] stmts, option.t[@ast.expr] expr) -> ast.block_ {
    auto index = new_str_hash[ast.block_index_entry]();
    for (@ast.stmt s in stmts) {
        ast.index_stmt(index, s);
    }
    ret rec(stmts=stmts, expr=expr, index=index, a=ast.ann_none);
}

fn index_arm(@ast.pat pat) -> hashmap[ast.ident,ast.def_id] {
    fn do_index_arm(&hashmap[ast.ident,ast.def_id] index, @ast.pat pat) {
        alt (pat.node) {
            case (ast.pat_bind(?i, ?def_id, _)) { index.insert(i, def_id); }
            case (ast.pat_wild(_)) { /* empty */ }
            case (ast.pat_lit(_, _)) { /* empty */ }
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
        case (ast.stmt_expr(?e,_)) { ret some[@ast.expr](e); }
        case (_) { /* fall through */ }
    }
    ret none[@ast.expr];
}

fn stmt_ends_with_semi(@ast.stmt stmt) -> bool {
    alt (stmt.node) {
        case (ast.stmt_decl(?d,_)) {
            alt (d.node) {
                case (ast.decl_local(_)) { ret true; }
                case (ast.decl_item(_)) { ret false; }
            }
        }
        case (ast.stmt_expr(?e,_)) {
            alt (e.node) {
                case (ast.expr_vec(_,_,_))      { ret true; }
                case (ast.expr_tup(_,_))        { ret true; }
                case (ast.expr_rec(_,_,_))      { ret true; }
                case (ast.expr_call(_,_,_))     { ret true; }
                case (ast.expr_self_method(_,_)){ ret false; }
                case (ast.expr_binary(_,_,_,_)) { ret true; }
                case (ast.expr_unary(_,_,_))    { ret true; }
                case (ast.expr_lit(_,_))        { ret true; }
                case (ast.expr_cast(_,_,_))     { ret true; }
                case (ast.expr_if(_,_,_,_))     { ret false; }
                case (ast.expr_for(_,_,_,_))    { ret false; }
                case (ast.expr_for_each(_,_,_,_))
                    { ret false; }
                case (ast.expr_while(_,_,_))    { ret false; }
                case (ast.expr_do_while(_,_,_)) { ret false; }
                case (ast.expr_alt(_,_,_))      { ret false; }
                case (ast.expr_block(_,_))      { ret false; }
                case (ast.expr_assign(_,_,_))   { ret true; }
                case (ast.expr_assign_op(_,_,_,_))
                    { ret true; }
                case (ast.expr_send(_,_,_))     { ret true; }
                case (ast.expr_recv(_,_,_))     { ret true; }
                case (ast.expr_field(_,_,_))    { ret true; }
                case (ast.expr_index(_,_,_))    { ret true; }
                case (ast.expr_path(_,_,_))     { ret true; }
                case (ast.expr_fail(_))         { ret true; }
                case (ast.expr_break(_))        { ret true; }
                case (ast.expr_cont(_))         { ret true; }
                case (ast.expr_ret(_,_))        { ret true; }
                case (ast.expr_put(_,_))        { ret true; }
                case (ast.expr_be(_,_))         { ret true; }
                case (ast.expr_log(_,_,_))        { ret true; }
                case (ast.expr_check_expr(_,_)) { ret true; }
            }
        }
        // We should not be calling this on a cdir.
        case (ast.stmt_crate_directive(?cdir))  { fail; }
    }
}

fn parse_block(parser p) -> ast.block {
    auto lo = p.get_lo_pos();

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
                        // FIXME: crazy differentiation between conditions
                        // used in branches and binary expressions in rustboot
                        // means we cannot use && here. I know, right?
                        if (p.get_file_type() == SOURCE_FILE) {
                            if (stmt_ends_with_semi(stmt)) {
                                expect(p, token.SEMI);
                            }
                        }
                    }
                }
            }
        }
    }

    auto hi = p.get_hi_pos();
    p.bump();

    auto bloc = index_block(stmts, expr);
    ret spanned[ast.block_](lo, hi, bloc);
}

fn parse_ty_param(parser p) -> ast.ty_param {
    ret parse_ident(p);
}

fn parse_ty_params(parser p) -> vec[ast.ty_param] {
    let vec[ast.ty_param] ty_params = vec();
    if (p.peek() == token.LBRACKET) {
        auto f = parse_ty_param;   // FIXME: pass as lval directly
        ty_params = parse_seq[ast.ty_param](token.LBRACKET, token.RBRACKET,
                                            some(token.COMMA), f, p).node;
    }
    ret ty_params;
}

fn parse_fn_decl(parser p) -> ast.fn_decl {
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

    // FIXME: dropping constrs on the floor at the moment.
    // pick them up when they're used by typestate pass.
    parse_constrs(p);

    if (p.peek() == token.RARROW) {
        p.bump();
        output = parse_ty(p);
    } else {
        output = @spanned(inputs.span.lo, inputs.span.hi, ast.ty_nil);
    }
    ret rec(inputs=inputs.node, output=output);
}

fn parse_fn(parser p, ast.proto proto) -> ast._fn {
    auto decl = parse_fn_decl(p);
    auto body = parse_block(p);
    ret rec(decl = decl,
            proto = proto,
            body = body);
}

fn parse_fn_header(parser p)
    -> tup(ast.ident, vec[ast.ty_param]) {
    auto id = parse_ident(p);
    auto ty_params = parse_ty_params(p);
    ret tup(id, ty_params);
}

fn parse_item_fn_or_iter(parser p) -> @ast.item {
    auto lo = p.get_lo_pos();
    auto proto = parse_proto(p);
    auto t = parse_fn_header(p);
    auto f = parse_fn(p, proto);
    auto item = ast.item_fn(t._0, f, t._1,
                            p.next_def_id(), ast.ann_none);
    ret @spanned(lo, f.body.span.hi, item);
}


fn parse_obj_field(parser p) -> ast.obj_field {
    auto mut = parse_mutability(p); // TODO: store this, use it in typeck
    auto ty = parse_ty(p);
    auto ident = parse_ident(p);
    ret rec(ty=ty, ident=ident, id=p.next_def_id(), ann=ast.ann_none);
}

fn parse_method(parser p) -> @ast.method {
    auto lo = p.get_lo_pos();
    auto proto = parse_proto(p);
    auto ident = parse_ident(p);
    auto f = parse_fn(p, proto);
    auto meth = rec(ident=ident, meth=f,
                    id=p.next_def_id(), ann=ast.ann_none);
    ret @spanned(lo, f.body.span.hi, meth);
}

fn parse_dtor(parser p) -> @ast.method {
    auto lo = p.get_lo_pos();
    expect(p, token.DROP);
    let ast.block b = parse_block(p);
    let vec[ast.arg] inputs = vec();
    let @ast.ty output = @spanned(lo, lo, ast.ty_nil);
    let ast.fn_decl d = rec(inputs=inputs,
                            output=output);
    let ast._fn f = rec(decl = d,
                        proto = ast.proto_fn,
                        body = b);
    let ast.method_ m = rec(ident="drop",
                            meth=f,
                            id=p.next_def_id(),
                            ann=ast.ann_none);
    ret @spanned(lo, f.body.span.hi, m);
}

fn parse_item_obj(parser p, ast.layer lyr) -> @ast.item {
    auto lo = p.get_lo_pos();
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

    let vec[@ast.method] meths = vec();
    let option.t[@ast.method] dtor = none[@ast.method];

    expect(p, token.LBRACE);
    while (p.peek() != token.RBRACE) {
        alt (p.peek()) {
            case (token.DROP) {
                dtor = some[@ast.method](parse_dtor(p));
            }
            case (_) {
                _vec.push[@ast.method](meths,
                                       parse_method(p));
            }
        }
    }
    auto hi = p.get_hi_pos();
    expect(p, token.RBRACE);

    let ast._obj ob = rec(fields=fields.node,
                          methods=meths,
                          dtor=dtor);

    auto odid = rec(ty=p.next_def_id(), ctor=p.next_def_id());
    auto item = ast.item_obj(ident, ob, ty_params, odid, ast.ann_none);

    ret @spanned(lo, hi, item);
}

fn parse_mod_items(parser p, token.token term) -> ast._mod {
    auto index = new_str_hash[ast.mod_index_entry]();
    auto view_items = parse_view(p, index);
    let vec[@ast.item] items = vec();
    while (p.peek() != term) {
        auto item = parse_item(p);
        items += vec(item);

        // Index the item.
        ast.index_item(index, item);
    }
    ret rec(view_items=view_items, items=items, index=index);
}

fn parse_item_const(parser p) -> @ast.item {
    auto lo = p.get_lo_pos();
    expect(p, token.CONST);
    auto ty = parse_ty(p);
    auto id = parse_ident(p);
    expect(p, token.EQ);
    auto e = parse_expr(p);
    auto hi = p.get_hi_pos();
    expect(p, token.SEMI);
    auto item = ast.item_const(id, ty, e, p.next_def_id(), ast.ann_none);
    ret @spanned(lo, hi, item);
}

fn parse_item_mod(parser p) -> @ast.item {
    auto lo = p.get_lo_pos();
    expect(p, token.MOD);
    auto id = parse_ident(p);
    expect(p, token.LBRACE);
    auto m = parse_mod_items(p, token.RBRACE);
    auto hi = p.get_hi_pos();
    expect(p, token.RBRACE);
    auto item = ast.item_mod(id, m, p.next_def_id());
    ret @spanned(lo, hi, item);
}

fn parse_item_native_type(parser p) -> @ast.native_item {
    auto t = parse_type_decl(p);
    auto hi = p.get_hi_pos();
    expect(p, token.SEMI);
    auto item = ast.native_item_ty(t._1, p.next_def_id());
    ret @spanned(t._0, hi, item);
}

fn parse_item_native_fn(parser p) -> @ast.native_item {
    auto lo = p.get_lo_pos();
    expect(p, token.FN);
    auto t = parse_fn_header(p);
    auto decl = parse_fn_decl(p);
    auto link_name = none[str];
    if (p.peek() == token.EQ) {
        p.bump();
        link_name = some[str](parse_str_lit_or_env_ident(p));
    }
    auto hi = p.get_hi_pos();
    expect(p, token.SEMI);
    auto item = ast.native_item_fn(t._0, link_name, decl,
                                   t._1, p.next_def_id(),
                                   ast.ann_none);
    ret @spanned(lo, hi, item);
}

fn parse_native_item(parser p) -> @ast.native_item {
    let ast.layer lyr = parse_layer(p);
    alt (p.peek()) {
        case (token.TYPE) {
            ret parse_item_native_type(p);
        }
        case (token.FN) {
            ret parse_item_native_fn(p);
        }
        case (?t) {
            unexpected(p, t);
            fail;
        }
    }
}

fn parse_native_mod_items(parser p,
                                 str native_name,
                                 ast.native_abi abi) -> ast.native_mod {
    auto index = new_str_hash[ast.native_mod_index_entry]();
    let vec[@ast.native_item] items = vec();

    auto view_items = parse_native_view(p, index);

    while (p.peek() != token.RBRACE) {
        auto item = parse_native_item(p);
        items += vec(item);

        // Index the item.
        ast.index_native_item(index, item);
    }
    ret rec(native_name=native_name, abi=abi,
            view_items=view_items,
            items=items,
            index=index);
}

fn default_native_name(session.session sess, str id) -> str {
    alt (sess.get_targ_cfg().os) {
        case (session.os_win32) {
            ret id + ".dll";
        }
        case (session.os_macos) {
            ret "lib" + id + ".dylib";
        }
        case (session.os_linux) {
            ret "lib" + id + ".so";
        }
    }
}

fn parse_item_native_mod(parser p) -> @ast.item {
    auto lo = p.get_lo_pos();
    expect(p, token.NATIVE);
    auto abi = ast.native_abi_cdecl;
    if (p.peek() != token.MOD) {
        auto t = parse_str_lit_or_env_ident(p);
        if (_str.eq(t, "cdecl")) {
        } else if (_str.eq(t, "rust")) {
            abi = ast.native_abi_rust;
        } else if (_str.eq(t, "llvm")) {
            abi = ast.native_abi_llvm;
        } else {
            p.err("unsupported abi: " + t);
            fail;
        }
    }
    expect(p, token.MOD);
    auto id = parse_ident(p);
    auto native_name;
    if (p.peek() == token.EQ) {
        expect(p, token.EQ);
        native_name = parse_str_lit_or_env_ident(p);
    } else {
        native_name = default_native_name(p.get_session(), id);
    }
    expect(p, token.LBRACE);
    auto m = parse_native_mod_items(p, native_name, abi);
    auto hi = p.get_hi_pos();
    expect(p, token.RBRACE);
    auto item = ast.item_native_mod(id, m, p.next_def_id());
    ret @spanned(lo, hi, item);
}

fn parse_type_decl(parser p) -> tup(uint, ast.ident) {
    auto lo = p.get_lo_pos();
    expect(p, token.TYPE);
    auto id = parse_ident(p);
    ret tup(lo, id);
}

fn parse_item_type(parser p) -> @ast.item {
    auto t = parse_type_decl(p);
    auto tps = parse_ty_params(p);

    expect(p, token.EQ);
    auto ty = parse_ty(p);
    auto hi = p.get_hi_pos();
    expect(p, token.SEMI);
    auto item = ast.item_ty(t._1, ty, tps, p.next_def_id(), ast.ann_none);
    ret @spanned(t._0, hi, item);
}

fn parse_item_tag(parser p) -> @ast.item {
    auto lo = p.get_lo_pos();
    expect(p, token.TAG);
    auto id = parse_ident(p);
    auto ty_params = parse_ty_params(p);

    let vec[ast.variant] variants = vec();
    expect(p, token.LBRACE);
    while (p.peek() != token.RBRACE) {
        auto tok = p.peek();
        alt (tok) {
            case (token.IDENT(?name)) {
                auto vlo = p.get_lo_pos();
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

                auto vhi = p.get_hi_pos();
                expect(p, token.SEMI);

                auto id = p.next_def_id();
                auto vr = rec(name=name, args=args, id=id, ann=ast.ann_none);
                variants += vec(spanned[ast.variant_](vlo, vhi, vr));
            }
            case (token.RBRACE) { /* empty */ }
            case (_) {
                p.err("expected name of variant or '}' but found " +
                      token.to_str(tok));
            }
        }
    }
    auto hi = p.get_hi_pos();
    p.bump();

    auto item = ast.item_tag(id, variants, ty_params, p.next_def_id(),
                             ast.ann_none);
    ret @spanned(lo, hi, item);
}


fn parse_layer(parser p) -> ast.layer {
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


fn parse_auth(parser p) -> ast._auth {
    alt (p.peek()) {
        case (token.UNSAFE) {
            p.bump();
            ret ast.auth_unsafe;
        }
        case (?t) {
            unexpected(p, t);
        }
    }
    fail;
}

fn peeking_at_item(parser p) -> bool {
    alt (p.peek()) {
        case (token.STATE) { ret true; }
        case (token.GC) { ret true; }
        case (token.CONST) { ret true; }
        case (token.FN) { ret true; }
        case (token.ITER) { ret true; }
        case (token.MOD) { ret true; }
        case (token.TYPE) { ret true; }
        case (token.TAG) { ret true; }
        case (token.OBJ) { ret true; }
        case (_) { ret false; }
    }
    ret false;
}

fn parse_item(parser p) -> @ast.item {
    let ast.layer lyr = parse_layer(p);

    alt (p.peek()) {
        case (token.CONST) {
            check (lyr == ast.layer_value);
            ret parse_item_const(p);
        }

        case (token.FN) {
            check (lyr == ast.layer_value);
            ret parse_item_fn_or_iter(p);
        }
        case (token.ITER) {
            check (lyr == ast.layer_value);
            ret parse_item_fn_or_iter(p);
        }
        case (token.MOD) {
            check (lyr == ast.layer_value);
            ret parse_item_mod(p);
        }
        case (token.NATIVE) {
            check (lyr == ast.layer_value);
            ret parse_item_native_mod(p);
        }
        case (token.TYPE) {
            ret parse_item_type(p);
        }
        case (token.TAG) {
            ret parse_item_tag(p);
        }
        case (token.OBJ) {
            ret parse_item_obj(p, lyr);
        }
        case (?t) {
            p.err("expected item but found " + token.to_str(t));
        }
    }
    fail;
}

fn parse_meta_item(parser p) -> @ast.meta_item {
    auto lo = p.get_lo_pos();
    auto ident = parse_ident(p);
    expect(p, token.EQ);
    alt (p.peek()) {
        case (token.LIT_STR(?s)) {
            auto hi = p.get_hi_pos();
            p.bump();
            ret @spanned(lo, hi, rec(name = ident, value = s));
        }
        case (_) {
            p.err("Metadata items must be string literals");
        }
    }
    fail;
}

fn parse_meta(parser p) -> vec[@ast.meta_item] {
    auto pf = parse_meta_item;
    ret parse_seq[@ast.meta_item](token.LPAREN, token.RPAREN,
                                   some(token.COMMA), pf, p).node;
}

fn parse_optional_meta(parser p) -> vec[@ast.meta_item] {
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

fn parse_use(parser p) -> @ast.view_item {
    auto lo = p.get_lo_pos();
    expect(p, token.USE);
    auto ident = parse_ident(p);
    auto metadata = parse_optional_meta(p);
    auto hi = p.get_hi_pos();
    expect(p, token.SEMI);
    auto use_decl = ast.view_item_use(ident, metadata, p.next_def_id(),
                                      none[int]);
    ret @spanned(lo, hi, use_decl);
}

fn parse_rest_import_name(parser p, ast.ident first,
                                 option.t[ast.ident] def_ident)
        -> @ast.view_item {
    auto lo = p.get_lo_pos();
    let vec[ast.ident] identifiers = vec(first);
    while (p.peek() != token.SEMI) {
        expect(p, token.DOT);
        auto i = parse_ident(p);
        identifiers += vec(i);
    }
    auto hi = p.get_hi_pos();
    p.bump();
    auto defined_id;
    alt (def_ident) {
        case(some[ast.ident](?i)) {
            defined_id = i;
        }
        case (_) {
            auto len = _vec.len[ast.ident](identifiers);
            defined_id = identifiers.(len - 1u);
        }
    }
    auto import_decl = ast.view_item_import(defined_id, identifiers,
                                            p.next_def_id(),
                                            none[ast.def]);
    ret @spanned(lo, hi, import_decl);
}

fn parse_full_import_name(parser p, ast.ident def_ident)
       -> @ast.view_item {
    alt (p.peek()) {
        case (token.IDENT(?ident)) {
            p.bump();
            ret parse_rest_import_name(p, ident, some(def_ident));
        }
        case (_) {
            p.err("expecting an identifier");
        }
    }
    fail;
}

fn parse_import(parser p) -> @ast.view_item {
    expect(p, token.IMPORT);
    alt (p.peek()) {
        case (token.IDENT(?ident)) {
            p.bump();
            alt (p.peek()) {
                case (token.EQ) {
                    p.bump();
                    ret parse_full_import_name(p, ident);
                }
                case (_) {
                    ret parse_rest_import_name(p, ident, none[ast.ident]);
                }
            }
        }
        case (_) {
            p.err("expecting an identifier");
        }
    }
    fail;
}

fn parse_export(parser p) -> @ast.view_item {
    auto lo = p.get_lo_pos();
    expect(p, token.EXPORT);
    auto id = parse_ident(p);
    auto hi = p.get_hi_pos();
    expect(p, token.SEMI);
    ret @spanned(lo, hi, ast.view_item_export(id));
}

fn parse_view_item(parser p) -> @ast.view_item {
    alt (p.peek()) {
        case (token.USE) {
            ret parse_use(p);
        }
        case (token.IMPORT) {
            ret parse_import(p);
        }
        case (token.EXPORT) {
            ret parse_export(p);
        }
    }
}

fn is_view_item(token.token t) -> bool {
    alt (t) {
        case (token.USE) { ret true; }
        case (token.IMPORT) { ret true; }
        case (token.EXPORT) { ret true; }
        case (_) {}
    }
    ret false;
}

fn parse_view(parser p, ast.mod_index index) -> vec[@ast.view_item] {
    let vec[@ast.view_item] items = vec();
    while (is_view_item(p.peek())) {
        auto item = parse_view_item(p);
        items += vec(item);

        ast.index_view_item(index, item);
    }
    ret items;
}

fn parse_native_view(parser p, ast.native_mod_index index)
    -> vec[@ast.view_item] {
    let vec[@ast.view_item] items = vec();
    while (is_view_item(p.peek())) {
        auto item = parse_view_item(p);
        items += vec(item);

        ast.index_native_view_item(index, item);
    }
    ret items;
}


fn parse_crate_from_source_file(parser p) -> @ast.crate {
    auto lo = p.get_lo_pos();
    auto m = parse_mod_items(p, token.EOF);
    let vec[@ast.crate_directive] cdirs = vec();
    ret @spanned(lo, p.get_lo_pos(), rec(directives=cdirs,
                                         module=m));
}

// Logic for parsing crate files (.rc)
//
// Each crate file is a sequence of directives.
//
// Each directive imperatively extends its environment with 0 or more items.

fn parse_crate_directive(parser p) -> ast.crate_directive
{
    auto lo = p.get_lo_pos();
    alt (p.peek()) {
        case (token.AUTH) {
            p.bump();
            auto n = parse_path(p, GREEDY);
            expect(p, token.EQ);
            auto a = parse_auth(p);
            auto hi = p.get_hi_pos();
            expect(p, token.SEMI);
            ret spanned(lo, hi, ast.cdir_auth(n, a));
        }

        case (token.META) {
            p.bump();
            auto mis = parse_meta(p);
            auto hi = p.get_hi_pos();
            expect(p, token.SEMI);
            ret spanned(lo, hi, ast.cdir_meta(mis));
        }

        case (token.MOD) {
            p.bump();
            auto id = parse_ident(p);
            auto file_opt = none[filename];
            alt (p.peek()) {
                case (token.EQ) {
                    p.bump();
                    // FIXME: turn this into parse+eval expr
                    file_opt = some[filename](parse_str_lit_or_env_ident(p));
                }
                case (_) {}
            }


            alt (p.peek()) {

                // mod x = "foo.rs";

                case (token.SEMI) {
                    auto hi = p.get_hi_pos();
                    p.bump();
                    ret spanned(lo, hi, ast.cdir_src_mod(id, file_opt));
                }

                // mod x = "foo_dir" { ...directives... }

                case (token.LBRACE) {
                    p.bump();
                    auto cdirs = parse_crate_directives(p, token.RBRACE);
                    auto hi = p.get_hi_pos();
                    expect(p, token.RBRACE);
                    ret spanned(lo, hi,
                                ast.cdir_dir_mod(id, file_opt, cdirs));
                }

                case (?t) {
                    unexpected(p, t);
                }
            }
        }

        case (token.LET) {
            p.bump();
            expect(p, token.LPAREN);
            auto id = parse_ident(p);
            expect(p, token.EQ);
            auto x = parse_expr(p);
            expect(p, token.RPAREN);
            expect(p, token.LBRACE);
            auto v = parse_crate_directives(p, token.RBRACE);
            auto hi = p.get_hi_pos();
            expect(p, token.RBRACE);
            ret spanned(lo, hi, ast.cdir_let(id, x, v));
        }

        case (token.USE) {
            auto vi = parse_view_item(p);
            ret spanned(lo, vi.span.hi, ast.cdir_view_item(vi));
        }

        case (token.IMPORT) {
            auto vi = parse_view_item(p);
            ret spanned(lo, vi.span.hi, ast.cdir_view_item(vi));
        }

        case (token.EXPORT) {
            auto vi = parse_view_item(p);
            ret spanned(lo, vi.span.hi, ast.cdir_view_item(vi));
        }

        case (_) {
            auto x = parse_expr(p);
            ret spanned(lo, x.span.hi, ast.cdir_expr(x));
        }
    }
    fail;
}


fn parse_crate_directives(parser p, token.token term)
    -> vec[@ast.crate_directive] {

    let vec[@ast.crate_directive] cdirs = vec();

    while (p.peek() != term) {
        auto cdir = @parse_crate_directive(p);
        _vec.push[@ast.crate_directive](cdirs, cdir);
    }

    ret cdirs;
}

fn parse_crate_from_crate_file(parser p) -> @ast.crate {
    auto lo = p.get_lo_pos();
    auto prefix = std.fs.dirname(p.get_filemap().name);
    auto cdirs = parse_crate_directives(p, token.EOF);
    auto cx = @rec(p=p, sess=p.get_session(), mutable chpos=p.get_chpos());
    auto m = eval.eval_crate_directives_to_mod(cx, p.get_env(),
                                               cdirs, prefix);
    auto hi = p.get_hi_pos();
    expect(p, token.EOF);
    ret @spanned(lo, hi, rec(directives=cdirs,
                             module=m));
}


//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C $RBUILD 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
//
