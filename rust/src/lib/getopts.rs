/* Simple getopt alternative. Construct a vector of options, either by using
 * reqopt, optopt, and optflag or by building them from components yourself,
 * and pass them to getopts, along with a vector of actual arguments (not
 * including argv[0]). You'll either get a failure code back, or a match.
 * You'll have to verify whether the amount of 'free' arguments in the match
 * is what you expect. Use opt_* accessors (bottom of the file) to get
 * argument values out of the match object.
 */ 

import option::some;
import option::none;

tag name { long(str); short(char); }
tag hasarg { yes; no; maybe; }
tag occur { req; optional; multi; }

type opt = rec(name name, hasarg hasarg, occur occur);

fn mkname(str nm) -> name {
    if (_str::char_len(nm) == 1u) { ret short(_str::char_at(nm, 0u)); }
    else { ret long(nm); }
}
fn reqopt(str name) -> opt {
    ret rec(name=mkname(name), hasarg=yes, occur=req);
}
fn optopt(str name) -> opt {
    ret rec(name=mkname(name), hasarg=yes, occur=optional);
}
fn optflag(str name) -> opt {
    ret rec(name=mkname(name), hasarg=no, occur=optional);
}
fn optmulti(str name) -> opt {
    ret rec(name=mkname(name), hasarg=yes, occur=multi);
}

tag optval {
    val(str);
    given;
}

type match = rec(vec[opt] opts, vec[mutable vec[optval]] vals, vec[str] free);

fn is_arg(str arg) -> bool {
    ret _str::byte_len(arg) > 1u && arg.(0) == '-' as u8;
}
fn name_str(name nm) -> str {
    alt (nm) {
        case (short(?ch)) {ret _str::from_char(ch);}
        case (long(?s)) {ret s;}
    }
}

// FIXME rustboot workaround
fn name_eq(name a, name b) -> bool {
    alt (a) {
        case (long(?a)) {
            alt (b) {
                case (long(?b)) { ret _str::eq(a, b); }
                case (_) { ret false; }
            }
        }
        case (_) { if (a == b) { ret true; } else {ret false; } }
    }
}
fn find_opt(vec[opt] opts, name nm) -> option::t[uint] {
    auto i = 0u;
    auto l = _vec::len[opt](opts);
    while (i < l) {
        if (name_eq(opts.(i).name, nm)) { ret some[uint](i); }
        i += 1u;
    }
    ret none[uint];
}

tag fail_ {
    argument_missing(str);
    unrecognized_option(str);
    option_missing(str);
    option_duplicated(str);
}

fn fail_str(fail_ f) -> str {
    alt (f) {
        case (argument_missing(?nm)) {
            ret "Argument to option '" + nm + "' missing.";
        }
        case (unrecognized_option(?nm)) {
            ret "Unrecognized option: '" + nm + "'.";
        }
        case (option_missing(?nm)) {
            ret "Required option '" + nm + "' missing.";
        }
        case (option_duplicated(?nm)) {
            ret "option '" + nm + "' given more than once.";
        }
    }
}

tag result {
    success(match);
    failure(fail_);
}

fn getopts(vec[str] args, vec[opt] opts) -> result {
    auto n_opts = _vec::len[opt](opts);
    fn empty_(uint x) -> vec[optval]{ret _vec::empty[optval]();}
    auto f = empty_;
    auto vals = _vec::init_fn_mut[vec[optval]](f, n_opts);
    let vec[str] free = vec();

    auto l = _vec::len[str](args);
    auto i = 0u;
    while (i < l) {
        auto cur = args.(i);
        auto curlen = _str::byte_len(cur);
        if (!is_arg(cur)) {
            _vec::push[str](free, cur);
        } else if (_str::eq(cur, "--")) {
            free += _vec::slice[str](args, i + 1u, l);
            break;
        } else {
            auto names;
            auto i_arg = option::none[str];
            if (cur.(1) == '-' as u8) {
                auto tail = _str::slice(cur, 2u, curlen);
                auto eq = _str::index(tail, '=' as u8);
                if (eq == -1) {
                    names = vec(long(tail));
                } else {
                    names = vec(long(_str::slice(tail, 0u, eq as uint)));
                    i_arg = option::some[str]
                        (_str::slice(tail, (eq as uint) + 1u, curlen - 2u));
                }
            } else {
                auto j = 1u;
                names = vec();
                while (j < curlen) {
                    auto range = _str::char_range_at(cur, j);
                    _vec::push[name](names, short(range._0));
                    j = range._1;
                }
            }
            auto name_pos = 0u;
            for (name nm in names) {
                name_pos += 1u;
                auto optid;
                alt (find_opt(opts, nm)) {
                    case (some[uint](?id)) {optid = id;}
                    case (none[uint]) {
                        ret failure(unrecognized_option(name_str(nm)));
                    }
                }
                alt (opts.(optid).hasarg) {
                    case (no) {
                        _vec::push[optval](vals.(optid), given);
                    }
                    case (maybe) {
                        if (!option::is_none[str](i_arg)) {
                            _vec::push[optval](vals.(optid),
                                              val(option::get[str](i_arg)));
                        } else if (name_pos < _vec::len[name](names) ||
                                   i + 1u == l || is_arg(args.(i + 1u))) {
                            _vec::push[optval](vals.(optid), given);
                        } else {
                            i += 1u;
                            _vec::push[optval](vals.(optid), val(args.(i)));
                        }
                    }
                    case (yes) {
                        if (!option::is_none[str](i_arg)) {
                            _vec::push[optval](vals.(optid),
                                              val(option::get[str](i_arg)));
                        } else if (i + 1u == l) {
                            ret failure(argument_missing(name_str(nm)));
                        } else {
                            i += 1u;
                            _vec::push[optval](vals.(optid), val(args.(i)));
                        }
                    }
                }
            }
        }
        i += 1u;
    }

    i = 0u;
    while (i < n_opts) {
        auto n = _vec::len[optval](vals.(i));
        auto occ = opts.(i).occur;
        if (occ == req) {if (n == 0u) {
            ret failure(option_missing(name_str(opts.(i).name)));
        }}
        if (occ != multi) {if (n > 1u) {
            ret failure(option_duplicated(name_str(opts.(i).name)));
        }}
        i += 1u;
    }

    ret success(rec(opts=opts, vals=vals, free=free));
}

fn opt_vals(match m, str nm) -> vec[optval] {
    alt (find_opt(m.opts, mkname(nm))) {
        case (some[uint](?id)) { ret m.vals.(id); }
        case (none[uint]) {
            log_err "No option '" + nm + "' defined.";
            fail;
        }
    }
}
fn opt_val(match m, str nm) -> optval {
    ret opt_vals(m, nm).(0);
}
fn opt_present(match m, str nm) -> bool {
    ret _vec::len[optval](opt_vals(m, nm)) > 0u;
}
fn opt_str(match m, str nm) -> str {
    alt (opt_val(m, nm)) {
        case (val(?s)) { ret s; }
        case (_) { fail; }
    }
}
fn opt_strs(match m, str nm) -> vec[str] {
    let vec[str] acc = vec();
    for (optval v in opt_vals(m, nm)) {
        alt (v) {
            case (val(?s)) { _vec::push[str](acc, s); }
            case (_) {}
        }
    }
    ret acc;
}
fn opt_maybe_str(match m, str nm) -> option::t[str] {
    auto vals = opt_vals(m, nm);
    if (_vec::len[optval](vals) == 0u) { ret none[str]; }
    alt (vals.(0)) {
        case (val(?s)) { ret some[str](s); }
        case (_) { ret none[str]; }
    }
}

// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C .. 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
