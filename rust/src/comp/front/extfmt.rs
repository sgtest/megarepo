/*
 * The compiler code necessary to support the #fmt extension.  Eventually this
 * should all get sucked into either the standard library extfmt module or the
 * compiler syntax extension plugin interface.
 */

import front::parser::parser;
import util::common;

import std::_str;
import std::_vec;
import std::option;
import std::option::none;
import std::option::some;

import std::extfmt::ct::signedness;
import std::extfmt::ct::signed;
import std::extfmt::ct::unsigned;
import std::extfmt::ct::caseness;
import std::extfmt::ct::case_upper;
import std::extfmt::ct::case_lower;
import std::extfmt::ct::ty;
import std::extfmt::ct::ty_bool;
import std::extfmt::ct::ty_str;
import std::extfmt::ct::ty_char;
import std::extfmt::ct::ty_int;
import std::extfmt::ct::ty_bits;
import std::extfmt::ct::ty_hex;
import std::extfmt::ct::ty_octal;
import std::extfmt::ct::flag;
import std::extfmt::ct::flag_left_justify;
import std::extfmt::ct::flag_left_zero_pad;
import std::extfmt::ct::flag_space_for_sign;
import std::extfmt::ct::flag_sign_always;
import std::extfmt::ct::flag_alternate;
import std::extfmt::ct::count;
import std::extfmt::ct::count_is;
import std::extfmt::ct::count_is_param;
import std::extfmt::ct::count_is_next_param;
import std::extfmt::ct::count_implied;
import std::extfmt::ct::conv;
import std::extfmt::ct::piece;
import std::extfmt::ct::piece_string;
import std::extfmt::ct::piece_conv;
import std::extfmt::ct::parse_fmt_string;

export expand_syntax_ext;

fn expand_syntax_ext(parser p,
                     vec[@ast::expr] args,
                     option::t[str] body) -> @ast::expr {

    if (_vec::len[@ast::expr](args) == 0u) {
        // FIXME: Handle error correctly.
        log_err "malformed #fmt call";
        fail;
    }

    auto fmt = expr_to_str(args.(0));

    // log "Format string:";
    // log fmt;

    auto pieces = parse_fmt_string(fmt);
    auto args_len = _vec::len[@ast::expr](args);
    auto fmt_args = _vec::slice[@ast::expr](args, 1u, args_len - 1u);
    ret pieces_to_expr(p, pieces, args);
}

fn expr_to_str(@ast::expr expr) -> str {
    alt (expr.node) {
        case (ast::expr_lit(?l, _)) {
            alt (l.node) {
                case (ast::lit_str(?s)) {
                    ret s;
                }
            }
        }
    }
    // FIXME: Handle error correctly.
    log_err "malformed #fmt call";
    fail;
}

// FIXME: A lot of these functions for producing expressions can probably
// be factored out in common with other code that builds expressions.
// FIXME: Probably should be using the parser's span functions
// FIXME: Cleanup the naming of these functions
fn pieces_to_expr(parser p, vec[piece] pieces, vec[@ast::expr] args)
        -> @ast::expr {

    fn make_new_lit(parser p, common::span sp, ast::lit_ lit) -> @ast::expr {
        auto sp_lit = @rec(node=lit, span=sp);
        auto expr = ast::expr_lit(sp_lit, p.get_ann());
        ret @rec(node=expr, span=sp);
    }

    fn make_new_str(parser p, common::span sp, str s) -> @ast::expr {
        auto lit = ast::lit_str(s);
        ret make_new_lit(p, sp, lit);
    }

    fn make_new_int(parser p, common::span sp, int i) -> @ast::expr {
        auto lit = ast::lit_int(i);
        ret make_new_lit(p, sp, lit);
    }

    fn make_new_uint(parser p, common::span sp, uint u) -> @ast::expr {
        auto lit = ast::lit_uint(u);
        ret make_new_lit(p, sp, lit);
    }

    fn make_add_expr(parser p, common::span sp,
                     @ast::expr lhs, @ast::expr rhs) -> @ast::expr {
        auto binexpr = ast::expr_binary(ast::add, lhs, rhs, p.get_ann());
        ret @rec(node=binexpr, span=sp);
    }

    fn make_path_expr(parser p, common::span sp, vec[ast::ident] idents)
            -> @ast::expr {
        let vec[@ast::ty] types = vec();
        auto path = rec(idents=idents, types=types);
        auto sp_path = rec(node=path, span=sp);
        auto pathexpr = ast::expr_path(sp_path, p.get_ann());
        auto sp_pathexpr = @rec(node=pathexpr, span=sp);
        ret sp_pathexpr;
    }

    fn make_vec_expr(parser p, common::span sp, vec[@ast::expr] exprs)
            -> @ast::expr {
        auto vecexpr = ast::expr_vec(exprs, ast::imm, p.get_ann());
        auto sp_vecexpr = @rec(node=vecexpr, span=sp);
        ret sp_vecexpr;
    }

    fn make_call(parser p, common::span sp, vec[ast::ident] fn_path,
                 vec[@ast::expr] args) -> @ast::expr {
        auto pathexpr = make_path_expr(p, sp, fn_path);
        auto callexpr = ast::expr_call(pathexpr, args, p.get_ann());
        auto sp_callexpr = @rec(node=callexpr, span=sp);
        ret sp_callexpr;
    }

    fn make_rec_expr(parser p, common::span sp,
                     vec[tup(ast::ident, @ast::expr)] fields) -> @ast::expr {
        let vec[ast::field] astfields = vec();
        for (tup(ast::ident, @ast::expr) field in fields) {
            auto ident = field._0;
            auto val = field._1;
            auto astfield = rec(mut = ast::imm,
                                ident = ident,
                                expr = val);
            astfields += vec(astfield);
        }

        auto recexpr = ast::expr_rec(astfields,
                                    option::none[@ast::expr],
                                    p.get_ann());
        auto sp_recexpr = @rec(node=recexpr, span=sp);
        ret sp_recexpr;
    }

    fn make_path_vec(str ident) -> vec[str] {
        // FIXME: #fmt can't currently be used from within std
        // because we're explicitly referencing the 'std' crate here
        ret vec("std", "extfmt", "rt", ident);
    }

    fn make_rt_path_expr(parser p, common::span sp, str ident) -> @ast::expr {
        auto path = make_path_vec(ident);
        ret make_path_expr(p, sp, path);
    }

    // Produces an AST expression that represents a RT::conv record,
    // which tells the RT::conv* functions how to perform the conversion
    fn make_rt_conv_expr(parser p, common::span sp, &conv cnv) -> @ast::expr {

        fn make_flags(parser p, common::span sp, vec[flag] flags)
                -> @ast::expr {
            let vec[@ast::expr] flagexprs = vec();
            for (flag f in flags) {
                auto fstr;
                alt (f) {
                    case (flag_left_justify) {
                        fstr = "flag_left_justify";
                    }
                    case (flag_left_zero_pad) {
                        fstr = "flag_left_zero_pad";
                    }
                    case (flag_space_for_sign) {
                        fstr = "flag_space_for_sign";
                    }
                    case (flag_sign_always) {
                        fstr = "flag_sign_always";
                    }
                    case (flag_alternate) {
                        fstr = "flag_alternate";
                    }
                }
                flagexprs += vec(make_rt_path_expr(p, sp, fstr));
            }

            // FIXME: 0-length vectors can't have their type inferred
            // through the rec that these flags are a member of, so
            // this is a hack placeholder flag
            if (_vec::len[@ast::expr](flagexprs) == 0u) {
                flagexprs += vec(make_rt_path_expr(p, sp, "flag_none"));
            }

            ret make_vec_expr(p, sp, flagexprs);
        }

        fn make_count(parser p, common::span sp, &count cnt) -> @ast::expr {
            alt (cnt) {
                case (count_implied) {
                    ret make_rt_path_expr(p, sp, "count_implied");
                }
                case (count_is(?c)) {
                    auto count_lit = make_new_int(p, sp, c);
                    auto count_is_path = make_path_vec("count_is");
                    auto count_is_args = vec(count_lit);
                    ret make_call(p, sp, count_is_path, count_is_args);
                }
                case (_) {
                    log_err "not implemented";
                    fail;
                }
            }
        }

        fn make_ty(parser p, common::span sp, &ty t) -> @ast::expr {
            auto rt_type;
            alt (t) {
                case (ty_hex(?c)) {
                    alt (c) {
                        case (case_upper) {
                            rt_type = "ty_hex_upper";
                        }
                        case (case_lower) {
                            rt_type = "ty_hex_lower";
                        }
                    }
                }
                case (ty_bits) {
                    rt_type = "ty_bits";
                }
                case (ty_octal) {
                    rt_type = "ty_octal";
                }
                case (_) {
                    rt_type = "ty_default";
                }
            }

            ret make_rt_path_expr(p, sp, rt_type);
        }

        fn make_conv_rec(parser p,
                         common::span sp,
                         @ast::expr flags_expr,
                         @ast::expr width_expr,
                         @ast::expr precision_expr,
                         @ast::expr ty_expr) -> @ast::expr {
            ret make_rec_expr(p, sp, vec(tup("flags", flags_expr),
                                         tup("width", width_expr),
                                         tup("precision", precision_expr),
                                         tup("ty", ty_expr)));
        }

        auto rt_conv_flags = make_flags(p, sp, cnv.flags);
        auto rt_conv_width = make_count(p, sp, cnv.width);
        auto rt_conv_precision = make_count(p, sp, cnv.precision);
        auto rt_conv_ty = make_ty(p, sp, cnv.ty);
        ret make_conv_rec(p,
                          sp,
                          rt_conv_flags,
                          rt_conv_width,
                          rt_conv_precision,
                          rt_conv_ty);
    }

    fn make_conv_call(parser p, common::span sp, str conv_type,
                      &conv cnv, @ast::expr arg) -> @ast::expr {
        auto fname = "conv_" + conv_type;
        auto path = make_path_vec(fname);
        auto cnv_expr = make_rt_conv_expr(p, sp, cnv);
        auto args = vec(cnv_expr, arg);
        ret make_call(p, arg.span, path, args);
    }

    fn make_new_conv(parser p, conv cnv, @ast::expr arg) -> @ast::expr {

        // FIXME: Extract all this validation into extfmt::ct
        fn is_signed_type(conv cnv) -> bool {
            alt (cnv.ty) {
                case (ty_int(?s)) {
                    alt (s) {
                        case (signed) {
                            ret true;
                        }
                        case (unsigned) {
                            ret false;
                        }
                    }
                }
                case (_) {
                    ret false;
                }
            }
        }

        auto unsupported = "conversion not supported in #fmt string";

        alt (cnv.param) {
            case (option::none[int]) {
            }
            case (_) {
                log_err unsupported;
                fail;
            }
        }

        for (flag f in cnv.flags) {
            alt (f) {
                case (flag_left_justify) {
                }
                case (flag_sign_always) {
                    if (!is_signed_type(cnv)) {
                        log_err "+ flag only valid in signed #fmt conversion";
                        fail;
                    }
                }
                case (flag_space_for_sign) {
                    if (!is_signed_type(cnv)) {
                        log_err "space flag only valid in "
                            + "signed #fmt conversions";
                        fail;
                    }
                }
                case (flag_left_zero_pad) {
                }
                case (_) {
                    log_err unsupported;
                    fail;
                }
            }
        }

        alt (cnv.width) {
            case (count_implied) {
            }
            case (count_is(_)) {
            }
            case (_) {
                log_err unsupported;
                fail;
            }
        }

        alt (cnv.precision) {
            case (count_implied) {
            }
            case (count_is(_)) {
            }
            case (_) {
                log_err unsupported;
                fail;
            }
        }

        alt (cnv.ty) {
            case (ty_str) {
                ret make_conv_call(p, arg.span, "str", cnv, arg);
            }
            case (ty_int(?sign)) {
                alt (sign) {
                    case (signed) {
                        ret make_conv_call(p, arg.span, "int", cnv, arg);
                    }
                    case (unsigned) {
                        ret make_conv_call(p, arg.span, "uint", cnv, arg);
                    }
                }
            }
            case (ty_bool) {
                ret make_conv_call(p, arg.span, "bool", cnv, arg);
            }
            case (ty_char) {
                ret make_conv_call(p, arg.span, "char", cnv, arg);
            }
            case (ty_hex(_)) {
                ret make_conv_call(p, arg.span, "uint", cnv, arg);
            }
            case (ty_bits) {
                ret make_conv_call(p, arg.span, "uint", cnv, arg);
            }
            case (ty_octal) {
                ret make_conv_call(p, arg.span, "uint", cnv, arg);
            }
            case (_) {
                log_err unsupported;
                fail;
            }
        }
    }

    fn log_conv(conv c) {
        alt (c.param) {
            case (some[int](?p)) {
                log "param: " + std::_int::to_str(p, 10u);
            }
            case (_) {
                log "param: none";
            }
        }
        for (flag f in c.flags) {
            alt (f) {
                case (flag_left_justify) {
                    log "flag: left justify";
                }
                case (flag_left_zero_pad) {
                    log "flag: left zero pad";
                }
                case (flag_space_for_sign) {
                    log "flag: left space pad";
                }
                case (flag_sign_always) {
                    log "flag: sign always";
                }
                case (flag_alternate) {
                    log "flag: alternate";
                }
            }
        }
        alt (c.width) {
            case (count_is(?i)) {
                log "width: count is " + std::_int::to_str(i, 10u);
            }
            case (count_is_param(?i)) {
                log "width: count is param " + std::_int::to_str(i, 10u);
            }
            case (count_is_next_param) {
                log "width: count is next param";
            }
            case (count_implied) {
                log "width: count is implied";
            }
        }
        alt (c.precision) {
            case (count_is(?i)) {
                log "prec: count is " + std::_int::to_str(i, 10u);
            }
            case (count_is_param(?i)) {
                log "prec: count is param " + std::_int::to_str(i, 10u);
            }
            case (count_is_next_param) {
                log "prec: count is next param";
            }
            case (count_implied) {
                log "prec: count is implied";
            }
        }
        alt (c.ty) {
            case (ty_bool) {
                log "type: bool";
            }
            case (ty_str) {
                log "type: str";
            }
            case (ty_char) {
                log "type: char";
            }
            case (ty_int(?s)) {
                alt (s) {
                    case (signed) {
                        log "type: signed";
                    }
                    case (unsigned) {
                        log "type: unsigned";
                    }
                }
            }
            case (ty_bits) {
                log "type: bits";
            }
            case (ty_hex(?cs)) {
                alt (cs) {
                    case (case_upper) {
                        log "type: uhex";
                    }
                    case (case_lower) {
                        log "type: lhex";
                    }
                }
            }
        }
    }

    auto sp = args.(0).span;
    auto n = 0u;
    auto tmp_expr = make_new_str(p, sp, "");

    for (piece pc in pieces) {
        alt (pc) {
            case (piece_string(?s)) {
                auto s_expr = make_new_str(p, sp, s);
                tmp_expr = make_add_expr(p, sp, tmp_expr, s_expr);
            }
            case (piece_conv(?conv)) {
                if (n >= _vec::len[@ast::expr](args)) {
                    log_err "too many conversions in #fmt string";
                    fail;
                }

                // TODO: Remove debug logging
                //log "Building conversion:";
                //log_conv(conv);

                n += 1u;
                auto arg_expr = args.(n);
                auto c_expr = make_new_conv(p, conv, arg_expr);
                tmp_expr = make_add_expr(p, sp, tmp_expr, c_expr);
            }
        }
    }

    // TODO: Remove this debug logging
    // log "dumping expanded ast:";
    // log pretty::print_expr(tmp_expr);
    ret tmp_expr;
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
