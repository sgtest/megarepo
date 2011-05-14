import std::_str;
import std::_uint;
import std::_vec;
import std::box;
import std::ufind;
import std::map;
import std::map::hashmap;
import std::option;
import std::option::none;
import std::option::some;

import driver::session;
import front::ast;
import front::ast::mutability;
import front::creader;
import middle::metadata;
import util::common;

import util::common::ty_u8;
import util::common::ty_u16;
import util::common::ty_u32;
import util::common::ty_u64;

import util::common::ty_i8;
import util::common::ty_i16;
import util::common::ty_i32;
import util::common::ty_i64;

import util::common::ty_f32;
import util::common::ty_f64;

import util::common::new_def_hash;
import util::common::span;
import util::typestate_ann::ts_ann;

// Data types

tag mode {
    mo_val;
    mo_alias;
    mo_either;
}

type arg = rec(mode mode, t ty);
type field = rec(ast::ident ident, mt mt);
type method = rec(ast::proto proto,
                  ast::ident ident,
                  vec[arg] inputs,
                  t output);

type mt = rec(t ty, ast::mutability mut);

// Contains information needed to resolve types and (in the future) look up
// the types of AST nodes.
type creader_cache = hashmap[tup(int,uint,uint),ty::t];
type ctxt = rec(@type_store ts,
                session::session sess,
                resolve::def_map def_map,
                creader_cache rcache,
                hashmap[t,str] short_names_cache);
type ty_ctxt = ctxt;    // Needed for disambiguation from Unify::ctxt.

// Convert from method type to function type.  Pretty easy; we just drop
// 'ident'.
fn method_ty_to_fn_ty(ctxt cx, method m) -> t {
    ret mk_fn(cx, m.proto, m.inputs, m.output);
}

// Never construct these manually. These are interned.
//
// TODO: It'd be really nice to be able to hide this definition from the
// outside world, to enforce the above invariants.
type raw_t = rec(sty struct,
                 option::t[str] cname,
                 uint hash,
                 bool has_params,
                 bool has_bound_params,
                 bool has_vars,
                 bool has_locals);

type t = uint;

// NB: If you change this, you'll probably want to change the corresponding
// AST structure in front/ast::rs as well.
tag sty {
    ty_nil;
    ty_bool;
    ty_int;
    ty_float;
    ty_uint;
    ty_machine(util::common::ty_mach);
    ty_char;
    ty_str;
    ty_tag(ast::def_id, vec[t]);
    ty_box(mt);
    ty_vec(mt);
    ty_port(t);
    ty_chan(t);
    ty_task;
    ty_tup(vec[mt]);
    ty_rec(vec[field]);
    ty_fn(ast::proto, vec[arg], t);
    ty_native_fn(ast::native_abi, vec[arg], t);
    ty_obj(vec[method]);
    ty_var(int);                                    // ephemeral type var
    ty_local(ast::def_id);                           // type of a local var
    ty_param(uint);                                 // fn/tag type param
    ty_bound_param(uint);                           // bound param, only paths
    ty_type;
    ty_native;
    // TODO: ty_fn_arg(t), for a possibly-aliased function argument
}

// Data structures used in type unification

type unify_handler = obj {
    fn resolve_local(ast::def_id id) -> option::t[t];
    fn record_local(ast::def_id id, t ty);  // TODO: -> Unify::result
    fn record_param(uint index, t binding) -> Unify::result;
};

tag type_err {
    terr_mismatch;
    terr_box_mutability;
    terr_vec_mutability;
    terr_tuple_size(uint, uint);
    terr_tuple_mutability;
    terr_record_size(uint, uint);
    terr_record_mutability;
    terr_record_fields(ast::ident,ast::ident);
    terr_meth_count;
    terr_obj_meths(ast::ident,ast::ident);
    terr_arg_count;
}


type ty_param_count_and_ty = tup(uint, t);
type type_cache = hashmap[ast::def_id,ty_param_count_and_ty];

const uint idx_nil      = 0u;
const uint idx_bool     = 1u;
const uint idx_int      = 2u;
const uint idx_float    = 3u;
const uint idx_uint     = 4u;
const uint idx_i8       = 5u;
const uint idx_i16      = 6u;
const uint idx_i32      = 7u;
const uint idx_i64      = 8u;
const uint idx_u8       = 9u;
const uint idx_u16      = 10u;
const uint idx_u32      = 11u;
const uint idx_u64      = 12u;
const uint idx_f32      = 13u;
const uint idx_f64      = 14u;
const uint idx_char     = 15u;
const uint idx_str      = 16u;
const uint idx_task     = 17u;
const uint idx_native   = 18u;
const uint idx_type     = 19u;
const uint idx_first_others = 20u;

type type_store = rec(mutable vec[raw_t] others,
                      hashmap[raw_t,uint] other_structural);

type ty_param_substs_opt_and_ty = tup(option::t[vec[ty::t]], ty::t);
type node_type_table =
    @mutable vec[mutable option::t[ty::ty_param_substs_opt_and_ty]];

fn mk_type_store() -> @type_store {
    let vec[raw_t] others = vec();
    let hashmap[raw_t,uint] ost =
        map::mk_hashmap[raw_t,uint](hash_raw_ty, eq_raw_ty);

    auto ts = @rec(mutable others=others, other_structural=ost);

    intern(ts, ty_nil, none[str]);
    intern(ts, ty_bool, none[str]);
    intern(ts, ty_int, none[str]);
    intern(ts, ty_float, none[str]);
    intern(ts, ty_uint, none[str]);
    intern(ts, ty_machine(ty_i8), none[str]);
    intern(ts, ty_machine(ty_i16), none[str]);
    intern(ts, ty_machine(ty_i32), none[str]);
    intern(ts, ty_machine(ty_i64), none[str]);
    intern(ts, ty_machine(ty_u8), none[str]);
    intern(ts, ty_machine(ty_u16), none[str]);
    intern(ts, ty_machine(ty_u32), none[str]);
    intern(ts, ty_machine(ty_u64), none[str]);
    intern(ts, ty_machine(ty_f32), none[str]);
    intern(ts, ty_machine(ty_f64), none[str]);
    intern(ts, ty_char, none[str]);
    intern(ts, ty_str, none[str]);
    intern(ts, ty_task, none[str]);
    intern(ts, ty_native, none[str]);
    intern(ts, ty_type, none[str]);

    assert _vec::len(ts.others) == idx_first_others;

    ret ts;
}

fn mk_rcache() -> creader_cache {
    fn hash_cache_entry(&tup(int,uint,uint) k) -> uint {
        ret (k._0 as uint) + k._1 + k._2;
    }
    fn eq_cache_entries(&tup(int,uint,uint) a,
                        &tup(int,uint,uint) b) -> bool {
        ret a._0 == b._0 &&
            a._1 == b._1 &&
            a._2 == b._2;
    }
    auto h = hash_cache_entry;
    auto e = eq_cache_entries;
    ret map::mk_hashmap[tup(int,uint,uint),t](h, e);
}

fn mk_ctxt(session::session s, resolve::def_map dm) -> ctxt {
    ret rec(ts = mk_type_store(),
            sess = s,
            def_map = dm,
            rcache = mk_rcache(),
            short_names_cache =
                map::mk_hashmap[ty::t,str](ty::hash_ty, ty::eq_ty));
}


// Type constructors

fn mk_raw_ty(&@type_store ts, &sty st, &option::t[str] cname) -> raw_t {
    auto h = hash_type_info(st, cname);

    let bool has_params = false;
    let bool has_bound_params = false;
    let bool has_vars = false;
    let bool has_locals = false;

    fn derive_flags_t(@type_store ts,
                      &mutable bool has_params,
                      &mutable bool has_bound_params,
                      &mutable bool has_vars,
                      &mutable bool has_locals,
                      &t tt) {
        auto rt = ts.others.(tt);
        has_params = has_params || rt.has_params;
        has_bound_params = has_bound_params || rt.has_bound_params;
        has_vars = has_vars || rt.has_vars;
        has_locals = has_locals || rt.has_locals;
    }

    fn derive_flags_mt(@type_store ts,
                       &mutable bool has_params,
                       &mutable bool has_bound_params,
                       &mutable bool has_vars,
                       &mutable bool has_locals,
                       &mt m) {
        derive_flags_t(ts, has_params, has_bound_params,
                       has_vars, has_locals, m.ty);
    }


    fn derive_flags_arg(@type_store ts,
                        &mutable bool has_params,
                        &mutable bool has_bound_params,
                        &mutable bool has_vars,
                        &mutable bool has_locals,
                        &arg a) {
        derive_flags_t(ts, has_params, has_bound_params,
                       has_vars, has_locals, a.ty);
    }

    fn derive_flags_sig(@type_store ts,
                        &mutable bool has_params,
                        &mutable bool has_bound_params,
                        &mutable bool has_vars,
                        &mutable bool has_locals,
                        &vec[arg] args,
                        &t tt) {
        for (arg a in args) {
            derive_flags_arg(ts, has_params, has_bound_params,
                             has_vars, has_locals, a);
        }
        derive_flags_t(ts, has_params, has_bound_params,
                       has_vars, has_locals, tt);
    }

    alt (st) {
        case (ty_param(_)) { has_params = true; }
        case (ty_bound_param(_)) { has_bound_params = true; }
        case (ty_var(_)) { has_vars = true; }
        case (ty_local(_)) { has_locals = true; }
        case (ty_tag(_, ?tys)) {
            for (t tt in tys) {
                derive_flags_t(ts, has_params, has_bound_params,
                               has_vars, has_locals, tt);
            }
        }
        case (ty_box(?m)) {
            derive_flags_mt(ts, has_params, has_bound_params,
                            has_vars, has_locals, m);
        }

        case (ty_vec(?m)) {
            derive_flags_mt(ts, has_params, has_bound_params,
                            has_vars, has_locals, m);
        }

        case (ty_port(?tt)) {
            derive_flags_t(ts, has_params, has_bound_params,
                           has_vars, has_locals, tt);
        }

        case (ty_chan(?tt)) {
            derive_flags_t(ts, has_params, has_bound_params,
                           has_vars, has_locals, tt);
        }

        case (ty_tup(?mts)) {
            for (mt m in mts) {
                derive_flags_mt(ts, has_params, has_bound_params,
                                has_vars, has_locals, m);
            }
        }

        case (ty_rec(?flds)) {
            for (field f in flds) {
                derive_flags_mt(ts, has_params, has_bound_params,
                                has_vars, has_locals, f.mt);
            }
        }

        case (ty_fn(_, ?args, ?tt)) {
            derive_flags_sig(ts, has_params, has_bound_params,
                             has_vars, has_locals, args, tt);
        }

        case (ty_native_fn(_, ?args, ?tt)) {
            derive_flags_sig(ts, has_params, has_bound_params,
                             has_vars, has_locals, args, tt);
        }

        case (ty_obj(?meths)) {
            for (method m in meths) {
                derive_flags_sig(ts, has_params, has_bound_params,
                                 has_vars, has_locals,
                                 m.inputs, m.output);
            }
        }
        case (_) { }
    }

    ret rec(struct=st, cname=cname, hash=h,
            has_params = has_params,
            has_bound_params = has_bound_params,
            has_vars = has_vars,
            has_locals = has_locals);
}

fn intern_raw_ty(&@type_store ts, &raw_t rt) {
    auto type_num = _vec::len[raw_t](ts.others);
    ts.others += vec(rt);
    ts.other_structural.insert(rt, type_num);
}

fn intern(&@type_store ts, &sty st, &option::t[str] cname) {
    intern_raw_ty(ts, mk_raw_ty(ts, st, cname));
}

fn gen_ty_full(&ctxt cx, &sty st, &option::t[str] cname) -> t {
    auto raw_type = mk_raw_ty(cx.ts, st, cname);

    // Is it interned?
    alt (cx.ts.other_structural.find(raw_type)) {
        case (some[t](?typ)) {
            ret typ;
        }
        case (none[t]) {
            // Nope: Insert it and return.
            auto type_num = _vec::len[raw_t](cx.ts.others);
            intern_raw_ty(cx.ts, raw_type);
            // log_err "added: " + ty_to_str(tystore, raw_type);
            ret type_num;
        }
    }
}

// These are private constructors to this module. External users should always
// use the mk_foo() functions below.
fn gen_ty(&ctxt cx, &sty st) -> t {
    ret gen_ty_full(cx, st, none[str]);
}

fn mk_nil(&ctxt cx) -> t          { ret idx_nil; }
fn mk_bool(&ctxt cx) -> t         { ret idx_bool; }
fn mk_int(&ctxt cx) -> t          { ret idx_int; }
fn mk_float(&ctxt cx) -> t        { ret idx_float; }
fn mk_uint(&ctxt cx) -> t         { ret idx_uint; }

fn mk_mach(&ctxt cx, &util::common::ty_mach tm) -> t {
    alt (tm) {
        case (ty_u8)  { ret idx_u8; }
        case (ty_u16) { ret idx_u16; }
        case (ty_u32) { ret idx_u32; }
        case (ty_u64) { ret idx_u64; }

        case (ty_i8)  { ret idx_i8; }
        case (ty_i16) { ret idx_i16; }
        case (ty_i32) { ret idx_i32; }
        case (ty_i64) { ret idx_i64; }

        case (ty_f32) { ret idx_f32; }
        case (ty_f64) { ret idx_f64; }
    }
    fail;
}

fn mk_char(&ctxt cx) -> t    { ret idx_char; }
fn mk_str(&ctxt cx) -> t     { ret idx_str; }

fn mk_tag(&ctxt cx, &ast::def_id did, &vec[t] tys) -> t {
    ret gen_ty(cx, ty_tag(did, tys));
}

fn mk_box(&ctxt cx, &mt tm) -> t {
    ret gen_ty(cx, ty_box(tm));
}

fn mk_imm_box(&ctxt cx, &t ty) -> t {
    ret mk_box(cx, rec(ty=ty, mut=ast::imm));
}

fn mk_vec(&ctxt cx, &mt tm) -> t  { ret gen_ty(cx, ty_vec(tm)); }

fn mk_imm_vec(&ctxt cx, &t typ) -> t {
    ret gen_ty(cx, ty_vec(rec(ty=typ, mut=ast::imm)));
}

fn mk_port(&ctxt cx, &t ty) -> t  { ret gen_ty(cx, ty_port(ty)); }
fn mk_chan(&ctxt cx, &t ty) -> t  { ret gen_ty(cx, ty_chan(ty)); }
fn mk_task(&ctxt cx) -> t        { ret gen_ty(cx, ty_task); }

fn mk_tup(&ctxt cx, &vec[mt] tms) -> t { ret gen_ty(cx, ty_tup(tms)); }

fn mk_imm_tup(&ctxt cx, &vec[t] tys) -> t {
    // TODO: map
    let vec[ty::mt] mts = vec();
    for (t typ in tys) {
        mts += vec(rec(ty=typ, mut=ast::imm));
    }
    ret mk_tup(cx, mts);
}

fn mk_rec(&ctxt cx, &vec[field] fs) -> t { ret gen_ty(cx, ty_rec(fs)); }

fn mk_fn(&ctxt cx, &ast::proto proto, &vec[arg] args, &t ty) -> t {
    ret gen_ty(cx, ty_fn(proto, args, ty));
}

fn mk_native_fn(&ctxt cx, &ast::native_abi abi, &vec[arg] args, &t ty) -> t {
    ret gen_ty(cx, ty_native_fn(abi, args, ty));
}

fn mk_obj(&ctxt cx, &vec[method] meths) -> t {
    ret gen_ty(cx, ty_obj(meths));
}

fn mk_var(&ctxt cx, int v) -> t {
    ret gen_ty(cx, ty_var(v));
}

fn mk_local(&ctxt cx, ast::def_id did) -> t {
    ret gen_ty(cx, ty_local(did));
}

fn mk_param(&ctxt cx, uint n) -> t {
    ret gen_ty(cx, ty_param(n));
}

fn mk_bound_param(&ctxt cx, uint n) -> t {
    ret gen_ty(cx, ty_bound_param(n));
}

fn mk_type(&ctxt cx) -> t    { ret idx_type; }
fn mk_native(&ctxt cx) -> t  { ret idx_native; }


// Returns the one-level-deep type structure of the given type.
fn struct(&ctxt cx, &t typ) -> sty { ret cx.ts.others.(typ).struct; }

// Returns the canonical name of the given type.
fn cname(&ctxt cx, &t typ) -> option::t[str] { ret cx.ts.others.(typ).cname; }


// Stringification

fn path_to_str(&ast::path pth) -> str {
    auto result = _str::connect(pth.node.idents,  "::");
    if (_vec::len[@ast::ty](pth.node.types) > 0u) {
        auto f = pretty::pprust::ty_to_str;
        result += "[";
        result += _str::connect(_vec::map(f, pth.node.types), ",");
        result += "]";
    }
    ret result;
}

fn ty_to_str(ctxt cx, &t typ) -> str {

    fn fn_input_to_str(ctxt cx, &rec(mode mode, t ty) input) -> str {
        auto s;
        alt (input.mode) {
            case (mo_val) { s = ""; }
            case (mo_alias) { s = "&"; }
            case (mo_either) { s = "?"; }
        }

        ret s + ty_to_str(cx, input.ty);
    }

    fn fn_to_str(ctxt cx,
                 ast::proto proto,
                 option::t[ast::ident] ident,
                 vec[arg] inputs, t output) -> str {
            auto f = bind fn_input_to_str(cx, _);

            auto s;
            alt (proto) {
                case (ast::proto_iter) {
                    s = "iter";
                }
                case (ast::proto_fn) {
                    s = "fn";
                }
            }

            alt (ident) {
                case (some[ast::ident](?i)) {
                    s += " ";
                    s += i;
                }
                case (_) { }
            }

            s += "(";
            s += _str::connect(_vec::map[arg,str](f, inputs), ", ");
            s += ")";

            if (struct(cx, output) != ty_nil) {
                s += " -> " + ty_to_str(cx, output);
            }
            ret s;
    }

    fn method_to_str(ctxt cx, &method m) -> str {
        ret fn_to_str(cx, m.proto, some[ast::ident](m.ident),
                      m.inputs, m.output) + ";";
    }

    fn field_to_str(ctxt cx, &field f) -> str {
        ret mt_to_str(cx, f.mt) + " " + f.ident;
    }

    fn mt_to_str(ctxt cx, &mt m) -> str {
        auto mstr;
        alt (m.mut) {
            case (ast::mut)       { mstr = "mutable "; }
            case (ast::imm)       { mstr = "";         }
            case (ast::maybe_mut) { mstr = "mutable? "; }
        }

        ret mstr + ty_to_str(cx, m.ty);
    }

    alt (cname(cx, typ)) {
        case (some[str](?cs)) {
            ret cs;
        }
        case (_) { }
    }

    auto s = "";

    alt (struct(cx, typ)) {
        case (ty_native)       { s += "native";                         }
        case (ty_nil)          { s += "()";                             }
        case (ty_bool)         { s += "bool";                           }
        case (ty_int)          { s += "int";                            }
        case (ty_float)        { s += "float";                          }
        case (ty_uint)         { s += "uint";                           }
        case (ty_machine(?tm)) { s += common::ty_mach_to_str(tm);        }
        case (ty_char)         { s += "char";                           }
        case (ty_str)          { s += "str";                            }
        case (ty_box(?tm))     { s += "@" + mt_to_str(cx, tm);          }
        case (ty_vec(?tm))     { s += "vec[" + mt_to_str(cx, tm) + "]"; }
        case (ty_port(?t))     { s += "port[" + ty_to_str(cx, t) + "]"; }
        case (ty_chan(?t))     { s += "chan[" + ty_to_str(cx, t) + "]"; }
        case (ty_type)         { s += "type";                           }

        case (ty_tup(?elems)) {
            auto f = bind mt_to_str(cx, _);
            auto strs = _vec::map[mt,str](f, elems);
            s += "tup(" + _str::connect(strs, ",") + ")";
        }

        case (ty_rec(?elems)) {
            auto f = bind field_to_str(cx, _);
            auto strs = _vec::map[field,str](f, elems);
            s += "rec(" + _str::connect(strs, ",") + ")";
        }

        case (ty_tag(?id, ?tps)) {
            // The user should never see this if the cname is set properly!
            s += "<tag#" + util::common::istr(id._0) + ":" +
                util::common::istr(id._1) + ">";
            if (_vec::len[t](tps) > 0u) {
                auto f = bind ty_to_str(cx, _);
                auto strs = _vec::map[t,str](f, tps);
                s += "[" + _str::connect(strs, ",") + "]";
            }
        }

        case (ty_fn(?proto, ?inputs, ?output)) {
            s += fn_to_str(cx, proto, none[ast::ident], inputs, output);
        }

        case (ty_native_fn(_, ?inputs, ?output)) {
            s += fn_to_str(cx, ast::proto_fn, none[ast::ident],
                           inputs, output);
        }

        case (ty_obj(?meths)) {
            auto f = bind method_to_str(cx, _);
            auto m = _vec::map[method,str](f, meths);
            s += "obj {\n\t" + _str::connect(m, "\n\t") + "\n}";
        }

        case (ty_var(?v)) {
            s += "<T" + util::common::istr(v) + ">";
        }

        case (ty_local(?id)) {
            s += "<L" + util::common::istr(id._0) + ":" +
                util::common::istr(id._1) + ">";
        }

        case (ty_param(?id)) {
            s += "'" + _str::unsafe_from_bytes(vec(('a' as u8) + (id as u8)));
        }

        case (ty_bound_param(?id)) {
            s += "''" + _str::unsafe_from_bytes(vec(('a' as u8) +
                                                    (id as u8)));
        }
    }

    ret s;
}

fn ty_to_short_str(ctxt cx, t typ) -> str {
    auto f = def_to_str;
    auto ecx = @rec(ds=f, tcx=cx, abbrevs=metadata::ac_no_abbrevs);
    auto s = metadata::Encode::ty_str(ecx, typ);
    if (_str::byte_len(s) >= 32u) { s = _str::substr(s, 0u, 32u); }
    ret s;
}

// Type folds

type ty_walk = fn(t);

fn walk_ty(ctxt cx, ty_walk walker, t ty) {
    alt (struct(cx, ty)) {
        case (ty_nil)           { /* no-op */ }
        case (ty_bool)          { /* no-op */ }
        case (ty_int)           { /* no-op */ }
        case (ty_uint)          { /* no-op */ }
        case (ty_float)         { /* no-op */ }
        case (ty_machine(_))    { /* no-op */ }
        case (ty_char)          { /* no-op */ }
        case (ty_str)           { /* no-op */ }
        case (ty_type)          { /* no-op */ }
        case (ty_native)        { /* no-op */ }
        case (ty_box(?tm))      { walk_ty(cx, walker, tm.ty); }
        case (ty_vec(?tm))      { walk_ty(cx, walker, tm.ty); }
        case (ty_port(?subty))  { walk_ty(cx, walker, subty); }
        case (ty_chan(?subty))  { walk_ty(cx, walker, subty); }
        case (ty_tag(?tid, ?subtys)) {
            for (t subty in subtys) {
                walk_ty(cx, walker, subty);
            }
        }
        case (ty_tup(?mts)) {
            for (mt tm in mts) {
                walk_ty(cx, walker, tm.ty);
            }
        }
        case (ty_rec(?fields)) {
            for (field fl in fields) {
                walk_ty(cx, walker, fl.mt.ty);
            }
        }
        case (ty_fn(?proto, ?args, ?ret_ty)) {
            for (arg a in args) {
                walk_ty(cx, walker, a.ty);
            }
            walk_ty(cx, walker, ret_ty);
        }
        case (ty_native_fn(?abi, ?args, ?ret_ty)) {
            for (arg a in args) {
                walk_ty(cx, walker, a.ty);
            }
            walk_ty(cx, walker, ret_ty);
        }
        case (ty_obj(?methods)) {
            let vec[method] new_methods = vec();
            for (method m in methods) {
                for (arg a in m.inputs) {
                    walk_ty(cx, walker, a.ty);
                }
                walk_ty(cx, walker, m.output);
            }
        }
        case (ty_var(_))         { /* no-op */ }
        case (ty_local(_))       { /* no-op */ }
        case (ty_param(_))       { /* no-op */ }
        case (ty_bound_param(_)) { /* no-op */ }
    }

    walker(ty);
}

type ty_fold = fn(t) -> t;

fn fold_ty(ctxt cx, ty_fold fld, t ty_0) -> t {
    auto ty = ty_0;
    alt (struct(cx, ty)) {
        case (ty_nil)           { /* no-op */ }
        case (ty_bool)          { /* no-op */ }
        case (ty_int)           { /* no-op */ }
        case (ty_uint)          { /* no-op */ }
        case (ty_float)         { /* no-op */ }
        case (ty_machine(_))    { /* no-op */ }
        case (ty_char)          { /* no-op */ }
        case (ty_str)           { /* no-op */ }
        case (ty_type)          { /* no-op */ }
        case (ty_native)        { /* no-op */ }
        case (ty_box(?tm)) {
            ty = copy_cname(cx, mk_box(cx, rec(ty=fold_ty(cx, fld, tm.ty),
                                               mut=tm.mut)), ty);
        }
        case (ty_vec(?tm)) {
            ty = copy_cname(cx, mk_vec(cx, rec(ty=fold_ty(cx, fld, tm.ty),
                                                          mut=tm.mut)), ty);
        }
        case (ty_port(?subty)) {
            ty = copy_cname(cx, mk_port(cx, fold_ty(cx, fld, subty)), ty);
        }
        case (ty_chan(?subty)) {
            ty = copy_cname(cx, mk_chan(cx, fold_ty(cx, fld, subty)), ty);
        }
        case (ty_tag(?tid, ?subtys)) {
            let vec[t] new_subtys = vec();
            for (t subty in subtys) {
                new_subtys += vec(fold_ty(cx, fld, subty));
            }
            ty = copy_cname(cx, mk_tag(cx, tid, new_subtys), ty);
        }
        case (ty_tup(?mts)) {
            let vec[mt] new_mts = vec();
            for (mt tm in mts) {
                auto new_subty = fold_ty(cx, fld, tm.ty);
                new_mts += vec(rec(ty=new_subty, mut=tm.mut));
            }
            ty = copy_cname(cx, mk_tup(cx, new_mts), ty);
        }
        case (ty_rec(?fields)) {
            let vec[field] new_fields = vec();
            for (field fl in fields) {
                auto new_ty = fold_ty(cx, fld, fl.mt.ty);
                auto new_mt = rec(ty=new_ty, mut=fl.mt.mut);
                new_fields += vec(rec(ident=fl.ident, mt=new_mt));
            }
            ty = copy_cname(cx, mk_rec(cx, new_fields), ty);
        }
        case (ty_fn(?proto, ?args, ?ret_ty)) {
            let vec[arg] new_args = vec();
            for (arg a in args) {
                auto new_ty = fold_ty(cx, fld, a.ty);
                new_args += vec(rec(mode=a.mode, ty=new_ty));
            }
            ty = copy_cname(cx, mk_fn(cx, proto, new_args,
                                      fold_ty(cx, fld, ret_ty)), ty);
        }
        case (ty_native_fn(?abi, ?args, ?ret_ty)) {
            let vec[arg] new_args = vec();
            for (arg a in args) {
                auto new_ty = fold_ty(cx, fld, a.ty);
                new_args += vec(rec(mode=a.mode, ty=new_ty));
            }
            ty = copy_cname(cx, mk_native_fn(cx, abi, new_args,
                                             fold_ty(cx, fld, ret_ty)), ty);
        }
        case (ty_obj(?methods)) {
            let vec[method] new_methods = vec();
            for (method m in methods) {
                let vec[arg] new_args = vec();
                for (arg a in m.inputs) {
                    new_args += vec(rec(mode=a.mode,
                                        ty=fold_ty(cx, fld, a.ty)));
                }
                new_methods += vec(rec(proto=m.proto, ident=m.ident,
                                       inputs=new_args,
                                       output=fold_ty(cx, fld, m.output)));
            }
            ty = copy_cname(cx, mk_obj(cx, new_methods), ty);
        }
        case (ty_var(_))         { /* no-op */ }
        case (ty_local(_))       { /* no-op */ }
        case (ty_param(_))       { /* no-op */ }
        case (ty_bound_param(_)) { /* no-op */ }
    }

    ret fld(ty);
}

// Type utilities

fn rename(ctxt cx, t typ, str new_cname) -> t {
    ret gen_ty_full(cx, struct(cx, typ), some[str](new_cname));
}

// Returns a type with the structural part taken from `struct_ty` and the
// canonical name from `cname_ty`.
fn copy_cname(ctxt cx, t struct_ty, t cname_ty) -> t {
    ret gen_ty_full(cx, struct(cx, struct_ty), cname(cx, cname_ty));
}

fn type_is_nil(&ctxt cx, &t ty) -> bool {
    alt (struct(cx, ty)) {
        case (ty_nil) { ret true; }
        case (_) { ret false; }
    }
    fail;
}

fn type_is_bool(&ctxt cx, &t ty) -> bool {
    alt (struct(cx, ty)) {
        case (ty_bool) { ret true; }
        case (_) { ret false; }
    }
}


fn type_is_structural(&ctxt cx, &t ty) -> bool {
    alt (struct(cx, ty)) {
        case (ty_tup(_))    { ret true; }
        case (ty_rec(_))    { ret true; }
        case (ty_tag(_,_))  { ret true; }
        case (ty_fn(_,_,_)) { ret true; }
        case (ty_obj(_))    { ret true; }
        case (_)            { ret false; }
    }
    fail;
}

fn type_is_sequence(&ctxt cx, &t ty) -> bool {
    alt (struct(cx, ty)) {
        case (ty_str)    { ret true; }
        case (ty_vec(_))    { ret true; }
        case (_)            { ret false; }
    }
    fail;
}

fn sequence_element_type(&ctxt cx, &t ty) -> t {
    alt (struct(cx, ty)) {
        case (ty_str)      { ret mk_mach(cx, common::ty_u8); }
        case (ty_vec(?mt)) { ret mt.ty; }
    }
    fail;
}


fn type_is_tup_like(&ctxt cx, &t ty) -> bool {
    alt (struct(cx, ty)) {
        case (ty_box(_))    { ret true; }
        case (ty_tup(_))    { ret true; }
        case (ty_rec(_))    { ret true; }
        case (ty_tag(_,_))  { ret true; }
        case (_)            { ret false; }
    }
    fail;
}

fn get_element_type(&ctxt cx, &t ty, uint i) -> t {
    assert (type_is_tup_like(cx, ty));
    alt (struct(cx, ty)) {
        case (ty_tup(?mts)) {
            ret mts.(i).ty;
        }
        case (ty_rec(?flds)) {
            ret flds.(i).mt.ty;
        }
    }
    fail;
}

fn type_is_box(&ctxt cx, &t ty) -> bool {
    alt (struct(cx, ty)) {
        case (ty_box(_)) { ret true; }
        case (_) { ret false; }
    }
    fail;
}

fn type_is_boxed(&ctxt cx, &t ty) -> bool {
    alt (struct(cx, ty)) {
        case (ty_str) { ret true; }
        case (ty_vec(_)) { ret true; }
        case (ty_box(_)) { ret true; }
        case (ty_port(_)) { ret true; }
        case (ty_chan(_)) { ret true; }
        case (_) { ret false; }
    }
    fail;
}

fn type_is_scalar(&ctxt cx, &t ty) -> bool {
    alt (struct(cx, ty)) {
        case (ty_nil) { ret true; }
        case (ty_bool) { ret true; }
        case (ty_int) { ret true; }
        case (ty_float) { ret true; }
        case (ty_uint) { ret true; }
        case (ty_machine(_)) { ret true; }
        case (ty_char) { ret true; }
        case (ty_type) { ret true; }
        case (ty_native) { ret true; }
        case (_) { ret false; }
    }
    fail;
}

// FIXME: should we just return true for native types in
// type_is_scalar?
fn type_is_native(&ctxt cx, &t ty) -> bool {
    alt (struct(cx, ty)) {
        case (ty_native) { ret true; }
        case (_) { ret false; }
    }
    fail;
}

fn type_has_dynamic_size(&ctxt cx, &t ty) -> bool {
    alt (struct(cx, ty)) {
        case (ty_tup(?mts)) {
            auto i = 0u;
            while (i < _vec::len[mt](mts)) {
                if (type_has_dynamic_size(cx, mts.(i).ty)) { ret true; }
                i += 1u;
            }
        }
        case (ty_rec(?fields)) {
            auto i = 0u;
            while (i < _vec::len[field](fields)) {
                if (type_has_dynamic_size(cx, fields.(i).mt.ty)) {
                    ret true;
                }
                i += 1u;
            }
        }
        case (ty_tag(_, ?subtys)) {
            auto i = 0u;
            while (i < _vec::len[t](subtys)) {
                if (type_has_dynamic_size(cx, subtys.(i))) { ret true; }
                i += 1u;
            }
        }
        case (ty_param(_)) { ret true; }
        case (_) { /* fall through */ }
    }
    ret false;
}

fn type_is_integral(&ctxt cx, &t ty) -> bool {
    alt (struct(cx, ty)) {
        case (ty_int) { ret true; }
        case (ty_uint) { ret true; }
        case (ty_machine(?m)) {
            alt (m) {
                case (common::ty_i8) { ret true; }
                case (common::ty_i16) { ret true; }
                case (common::ty_i32) { ret true; }
                case (common::ty_i64) { ret true; }

                case (common::ty_u8) { ret true; }
                case (common::ty_u16) { ret true; }
                case (common::ty_u32) { ret true; }
                case (common::ty_u64) { ret true; }
                case (_) { ret false; }
            }
        }
        case (ty_char) { ret true; }
        case (_) { ret false; }
    }
    fail;
}

fn type_is_fp(&ctxt cx, &t ty) -> bool {
    alt (struct(cx, ty)) {
        case (ty_machine(?tm)) {
            alt (tm) {
                case (common::ty_f32) { ret true; }
                case (common::ty_f64) { ret true; }
                case (_) { ret false; }
            }
        }
        case (ty_float) {
            ret true;
        }
        case (_) { ret false; }
    }
    fail;
}

fn type_is_signed(&ctxt cx, &t ty) -> bool {
    alt (struct(cx, ty)) {
        case (ty_int) { ret true; }
        case (ty_machine(?tm)) {
            alt (tm) {
                case (common::ty_i8) { ret true; }
                case (common::ty_i16) { ret true; }
                case (common::ty_i32) { ret true; }
                case (common::ty_i64) { ret true; }
                case (_) { ret false; }
            }
        }
        case (_) { ret false; }
    }
    fail;
}

fn type_param(&ctxt cx, &t ty) -> option::t[uint] {
    alt (struct(cx, ty)) {
        case (ty_param(?id)) { ret some[uint](id); }
        case (_)             { /* fall through */  }
    }
    ret none[uint];
}

fn def_to_str(&ast::def_id did) -> str {
    ret #fmt("%d:%d", did._0, did._1);
}


// Type hashing. This function is private to this module (and slow); external
// users should use `hash_ty()` instead.
fn hash_type_structure(&sty st) -> uint {
    fn hash_uint(uint id, uint n) -> uint {
        auto h = id;
        h += h << 5u + n;
        ret h;
    }

    fn hash_def(uint id, ast::def_id did) -> uint {
        auto h = id;
        h += h << 5u + (did._0 as uint);
        h += h << 5u + (did._1 as uint);
        ret h;
    }

    fn hash_subty(uint id, &t subty) -> uint {
        auto h = id;
        h += h << 5u + hash_ty(subty);
        ret h;
    }

    fn hash_fn(uint id, &vec[arg] args, &t rty) -> uint {
        auto h = id;
        for (arg a in args) {
            h += h << 5u + hash_ty(a.ty);
        }
        h += h << 5u + hash_ty(rty);
        ret h;
    }

    alt (st) {
        case (ty_nil) { ret 0u; }
        case (ty_bool) { ret 1u; }
        case (ty_int) { ret 2u; }
        case (ty_float) { ret 3u; }
        case (ty_uint) { ret 4u; }
        case (ty_machine(?tm)) {
            alt (tm) {
                case (common::ty_i8) { ret 5u; }
                case (common::ty_i16) { ret 6u; }
                case (common::ty_i32) { ret 7u; }
                case (common::ty_i64) { ret 8u; }

                case (common::ty_u8) { ret 9u; }
                case (common::ty_u16) { ret 10u; }
                case (common::ty_u32) { ret 11u; }
                case (common::ty_u64) { ret 12u; }

                case (common::ty_f32) { ret 13u; }
                case (common::ty_f64) { ret 14u; }
            }
        }
        case (ty_char) { ret 15u; }
        case (ty_str) { ret 16u; }
        case (ty_tag(?did, ?tys)) {
            auto h = hash_def(17u, did);
            for (t typ in tys) {
                h += h << 5u + hash_ty(typ);
            }
            ret h;
        }
        case (ty_box(?mt)) { ret hash_subty(18u, mt.ty); }
        case (ty_vec(?mt)) { ret hash_subty(19u, mt.ty); }
        case (ty_port(?typ)) { ret hash_subty(20u, typ); }
        case (ty_chan(?typ)) { ret hash_subty(21u, typ); }
        case (ty_task) { ret 22u; }
        case (ty_tup(?mts)) {
            auto h = 23u;
            for (mt tm in mts) {
                h += h << 5u + hash_ty(tm.ty);
            }
            ret h;
        }
        case (ty_rec(?fields)) {
            auto h = 24u;
            for (field f in fields) {
                h += h << 5u + hash_ty(f.mt.ty);
            }
            ret h;
        }
        case (ty_fn(_, ?args, ?rty)) { ret hash_fn(25u, args, rty); }
        case (ty_native_fn(_, ?args, ?rty)) { ret hash_fn(26u, args, rty); }
        case (ty_obj(?methods)) {
            auto h = 27u;
            for (method m in methods) {
                h += h << 5u + _str::hash(m.ident);
            }
            ret h;
        }
        case (ty_var(?v)) { ret hash_uint(28u, v as uint); }
        case (ty_local(?did)) { ret hash_def(29u, did); }
        case (ty_param(?pid)) { ret hash_uint(30u, pid); }
        case (ty_bound_param(?pid)) { ret hash_uint(31u, pid); }
        case (ty_type) { ret 32u; }
        case (ty_native) { ret 33u; }
    }
}

fn hash_type_info(&sty st, &option::t[str] cname_opt) -> uint {
    auto h = hash_type_structure(st);
    alt (cname_opt) {
        case (none[str]) { /* no-op */ }
        case (some[str](?s)) { h += h << 5u + _str::hash(s); }
    }
    ret h;
}

fn hash_raw_ty(&raw_t rt) -> uint { ret rt.hash; }

fn hash_ty(&t typ) -> uint { ret typ; }


// Type equality. This function is private to this module (and slow); external
// users should use `eq_ty()` instead.
fn equal_type_structures(&sty a, &sty b) -> bool {
    fn equal_mt(&mt a, &mt b) -> bool {
        ret a.mut == b.mut && eq_ty(a.ty, b.ty);
    }

    fn equal_fn(&vec[arg] args_a, &t rty_a,
                &vec[arg] args_b, &t rty_b) -> bool {
        if (!eq_ty(rty_a, rty_b)) { ret false; }

        auto len = _vec::len[arg](args_a);
        if (len != _vec::len[arg](args_b)) { ret false; }

        auto i = 0u;
        while (i < len) {
            auto arg_a = args_a.(i); auto arg_b = args_b.(i);
            if (arg_a.mode != arg_b.mode) { ret false; }
            if (!eq_ty(arg_a.ty, arg_b.ty)) { ret false; }
            i += 1u;
        }
        ret true;
    }

    fn equal_def(&ast::def_id did_a, &ast::def_id did_b) -> bool {
        ret did_a._0 == did_b._0 && did_a._1 == did_b._1;
    }

    alt (a) {
        case (ty_nil) {
            alt (b) {
                case (ty_nil) { ret true; }
                case (_) { ret false; }
            }
        }
        case (ty_bool) {
            alt (b) {
                case (ty_bool) { ret true; }
                case (_) { ret false; }
            }
        }
        case (ty_int) {
            alt (b) {
                case (ty_int) { ret true; }
                case (_) { ret false; }
            }
        }
        case (ty_float) {
            alt (b) {
                case (ty_float) { ret true; }
                case (_) { ret false; }
            }
        }
        case (ty_uint) {
            alt (b) {
                case (ty_uint) { ret true; }
                case (_) { ret false; }
            }
        }
        case (ty_machine(?tm_a)) {
            alt (b) {
                case (ty_machine(?tm_b)) {
                    ret hash_type_structure(a) == hash_type_structure(b);
                }
                case (_) { ret false; }
            }
        }
        case (ty_char) {
            alt (b) {
                case (ty_char) { ret true; }
                case (_) { ret false; }
            }
        }
        case (ty_str) {
            alt (b) {
                case (ty_str) { ret true; }
                case (_) { ret false; }
            }
        }
        case (ty_tag(?id_a, ?tys_a)) {
            alt (b) {
                case (ty_tag(?id_b, ?tys_b)) {
                    if (!equal_def(id_a, id_b)) { ret false; }

                    auto len = _vec::len[t](tys_a);
                    if (len != _vec::len[t](tys_b)) { ret false; }
                    auto i = 0u;
                    while (i < len) {
                        if (!eq_ty(tys_a.(i), tys_b.(i))) { ret false; }
                        i += 1u;
                    }
                    ret true;
                }
                case (_) { ret false; }
            }
        }
        case (ty_box(?mt_a)) {
            alt (b) {
                case (ty_box(?mt_b)) { ret equal_mt(mt_a, mt_b); }
                case (_) { ret false; }
            }
        }
        case (ty_vec(?mt_a)) {
            alt (b) {
                case (ty_vec(?mt_b)) { ret equal_mt(mt_a, mt_b); }
                case (_) { ret false; }
            }
        }
        case (ty_port(?t_a)) {
            alt (b) {
                case (ty_port(?t_b)) { ret eq_ty(t_a, t_b); }
                case (_) { ret false; }
            }
        }
        case (ty_chan(?t_a)) {
            alt (b) {
                case (ty_chan(?t_b)) { ret eq_ty(t_a, t_b); }
                case (_) { ret false; }
            }
        }
        case (ty_task) {
            alt (b) {
                case (ty_task) { ret true; }
                case (_) { ret false; }
            }
        }
        case (ty_tup(?mts_a)) {
            alt (b) {
                case (ty_tup(?mts_b)) {
                    auto len = _vec::len[mt](mts_a);
                    if (len != _vec::len[mt](mts_b)) { ret false; }
                    auto i = 0u;
                    while (i < len) {
                        if (!equal_mt(mts_a.(i), mts_b.(i))) { ret false; }
                        i += 1u;
                    }
                    ret true;
                }
                case (_) { ret false; }
            }
        }
        case (ty_rec(?flds_a)) {
            alt (b) {
                case (ty_rec(?flds_b)) {
                    auto len = _vec::len[field](flds_a);
                    if (len != _vec::len[field](flds_b)) { ret false; }
                    auto i = 0u;
                    while (i < len) {
                        auto fld_a = flds_a.(i); auto fld_b = flds_b.(i);
                        if (!_str::eq(fld_a.ident, fld_b.ident) ||
                                !equal_mt(fld_a.mt, fld_b.mt)) {
                            ret false;
                        }
                        i += 1u;
                    }
                    ret true;
                }
                case (_) { ret false; }
            }
        }
        case (ty_fn(?p_a, ?args_a, ?rty_a)) {
            alt (b) {
                case (ty_fn(?p_b, ?args_b, ?rty_b)) {
                    ret p_a == p_b &&
                        equal_fn(args_a, rty_a, args_b, rty_b);
                }
                case (_) { ret false; }
            }
        }
        case (ty_native_fn(?abi_a, ?args_a, ?rty_a)) {
            alt (b) {
                case (ty_native_fn(?abi_b, ?args_b, ?rty_b)) {
                    ret abi_a == abi_b &&
                        equal_fn(args_a, rty_a, args_b, rty_b);
                }
                case (_) { ret false; }
            }
        }
        case (ty_obj(?methods_a)) {
            alt (b) {
                case (ty_obj(?methods_b)) {
                    auto len = _vec::len[method](methods_a);
                    if (len != _vec::len[method](methods_b)) { ret false; }
                    auto i = 0u;
                    while (i < len) {
                        auto m_a = methods_a.(i); auto m_b = methods_b.(i);
                        if (m_a.proto != m_b.proto ||
                                !_str::eq(m_a.ident, m_b.ident) ||
                                !equal_fn(m_a.inputs, m_a.output,
                                          m_b.inputs, m_b.output)) {
                            ret false;
                        }
                        i += 1u;
                    }
                    ret true;
                }
                case (_) { ret false; }
            }
        }
        case (ty_var(?v_a)) {
            alt (b) {
                case (ty_var(?v_b)) { ret v_a == v_b; }
                case (_) { ret false; }
            }
        }
        case (ty_local(?did_a)) {
            alt (b) {
                case (ty_local(?did_b)) { ret equal_def(did_a, did_b); }
                case (_) { ret false; }
            }
        }
        case (ty_param(?pid_a)) {
            alt (b) {
                case (ty_param(?pid_b)) { ret pid_a == pid_b; }
                case (_) { ret false; }
            }
        }
        case (ty_bound_param(?pid_a)) {
            alt (b) {
                case (ty_bound_param(?pid_b)) { ret pid_a == pid_b; }
                case (_) { ret false; }
            }
        }
        case (ty_type) {
            alt (b) {
                case (ty_type) { ret true; }
                case (_) { ret false; }
            }
        }
        case (ty_native) {
            alt (b) {
                case (ty_native) { ret true; }
                case (_) { ret false; }
            }
        }
    }
}

// An expensive type equality function. This function is private to this
// module.
//
// FIXME: Use structural comparison, but this loops forever and segfaults.
fn eq_raw_ty(&raw_t a, &raw_t b) -> bool {
    // Check hashes (fast path).
    if (a.hash != b.hash) {
        ret false;
    }

    // Check canonical names.
    alt (a.cname) {
        case (none[str]) {
            alt (b.cname) {
                case (none[str]) { /* ok */ }
                case (_) { ret false; }
            }
        }
        case (some[str](?s_a)) {
            alt (b.cname) {
                case (some[str](?s_b)) {
                    if (!_str::eq(s_a, s_b)) { ret false; }
                }
                case (_) { ret false; }
            }
        }
    }

    // Check structures.
    ret equal_type_structures(a.struct, b.struct);
}

// This is the equality function the public should use. It works as long as
// the types are interned.
fn eq_ty(&t a, &t b) -> bool { ret a == b; }


// Type lookups

fn ann_to_ty_param_substs_opt_and_ty(&node_type_table ntt, &ast::ann ann)
        -> ty_param_substs_opt_and_ty {
    alt (ann) {
        case (ast::ann_none(_)) {
            log_err "ann_to_ty_param_substs_opt_and_ty() called on a node " +
                "with no type params";
            fail;
        }
        case (ast::ann_type(_, ?t, ?tps, _)) { ret tup(tps, t); }
    }
}

fn ann_to_type(&node_type_table ntt, &ast::ann ann) -> t {
    ret ann_to_ty_param_substs_opt_and_ty(ntt, ann)._1;
}

fn ann_to_type_params(&node_type_table ntt, &ast::ann ann) -> vec[t] {
    alt (ann_to_ty_param_substs_opt_and_ty(ntt, ann)._0) {
        case (none[vec[t]]) {
            let vec[t] result = vec();
            ret result;
        }
        case (some[vec[t]](?tps)) { ret tps; }
    }
}

fn ann_has_type_params(&node_type_table ntt, &ast::ann ann) -> bool {
    auto tpt = ann_to_ty_param_substs_opt_and_ty(ntt, ann);
    ret !option::is_none[vec[t]](tpt._0);
}


// Returns the type of an annotation, with type parameter substitutions
// performed if applicable.
fn ann_to_monotype(ctxt cx, &node_type_table ntt, ast::ann a) -> t {
    auto tpot = ann_to_ty_param_substs_opt_and_ty(ntt, a);
    alt (tpot._0) {
        case (none[vec[t]]) { ret tpot._1; }
        case (some[vec[t]](?tps)) {
            ret substitute_type_params(cx, tps, tpot._1);
        }
    }
}


// Turns a type into an ann_type, using defaults for other fields.
fn triv_ann(uint node_id, t typ) -> ast::ann {
    ret ast::ann_type(node_id, typ, none[vec[t]], none[@ts_ann]);
}

// Creates a nil type annotation.
fn plain_ann(uint node_id, ctxt tcx) -> ast::ann {
    ret ast::ann_type(node_id, mk_nil(tcx), none[vec[ty::t]], none[@ts_ann]);
}


// Returns the number of distinct type parameters in the given type.
fn count_ty_params(ctxt cx, t ty) -> uint {
    fn counter(ctxt cx, @mutable vec[uint] param_indices, t ty) {
        alt (struct(cx, ty)) {
            case (ty_param(?param_idx)) {
                auto seen = false;
                for (uint other_param_idx in *param_indices) {
                    if (param_idx == other_param_idx) {
                        seen = true;
                    }
                }
                if (!seen) {
                    *param_indices += vec(param_idx);
                }
            }
            case (_) { /* fall through */ }
        }
    }

    let vec[uint] v = vec();    // FIXME: typechecker botch
    let @mutable vec[uint] param_indices = @mutable v;
    auto f = bind counter(cx, param_indices, _);
    walk_ty(cx, f, ty);
    ret _vec::len[uint](*param_indices);
}

fn type_contains_vars(&ctxt cx, &t typ) -> bool {
    ret cx.ts.others.(typ).has_vars;
}

fn type_contains_locals(&ctxt cx, &t typ) -> bool {
    ret cx.ts.others.(typ).has_locals;
}

fn type_contains_params(&ctxt cx, &t typ) -> bool {
    ret cx.ts.others.(typ).has_params;
}

fn type_contains_bound_params(&ctxt cx, &t typ) -> bool {
    ret cx.ts.others.(typ).has_bound_params;
}

// Type accessors for substructures of types

fn ty_fn_args(&ctxt cx, &t fty) -> vec[arg] {
    alt (struct(cx, fty)) {
        case (ty::ty_fn(_, ?a, _)) { ret a; }
        case (ty::ty_native_fn(_, ?a, _)) { ret a; }
    }
    fail;
}

fn ty_fn_proto(&ctxt cx, &t fty) -> ast::proto {
    alt (struct(cx, fty)) {
        case (ty::ty_fn(?p, _, _)) { ret p; }
    }
    fail;
}

fn ty_fn_abi(&ctxt cx, &t fty) -> ast::native_abi {
    alt (struct(cx, fty)) {
        case (ty::ty_native_fn(?a, _, _)) { ret a; }
    }
    fail;
}

fn ty_fn_ret(&ctxt cx, &t fty) -> t {
    alt (struct(cx, fty)) {
        case (ty::ty_fn(_, _, ?r)) { ret r; }
        case (ty::ty_native_fn(_, _, ?r)) { ret r; }
    }
    fail;
}

fn is_fn_ty(&ctxt cx, &t fty) -> bool {
    alt (struct(cx, fty)) {
        case (ty::ty_fn(_, _, _)) { ret true; }
        case (ty::ty_native_fn(_, _, _)) { ret true; }
        case (_) { ret false; }
    }
    ret false;
}


// Type accessors for AST nodes

// Given an item, returns the associated type as well as the number of type
// parameters it has.
fn native_item_ty(&node_type_table ntt, &@ast::native_item it)
        -> ty_param_count_and_ty {
    auto ty_param_count;
    auto result_ty;
    alt (it.node) {
        case (ast::native_item_fn(_, _, _, ?tps, _, ?ann)) {
            ty_param_count = _vec::len[ast::ty_param](tps);
            result_ty = ann_to_type(ntt, ann);
        }
    }
    ret tup(ty_param_count, result_ty);
}

fn item_ty(&node_type_table ntt, &@ast::item it) -> ty_param_count_and_ty {
    auto ty_param_count;
    auto result_ty;
    alt (it.node) {
        case (ast::item_const(_, _, _, _, ?ann)) {
            ty_param_count = 0u;
            result_ty = ann_to_type(ntt, ann);
        }
        case (ast::item_fn(_, _, ?tps, _, ?ann)) {
            ty_param_count = _vec::len[ast::ty_param](tps);
            result_ty = ann_to_type(ntt, ann);
        }
        case (ast::item_mod(_, _, _)) {
            fail;   // modules are typeless
        }
        case (ast::item_ty(_, _, ?tps, _, ?ann)) {
            ty_param_count = _vec::len[ast::ty_param](tps);
            result_ty = ann_to_type(ntt, ann);
        }
        case (ast::item_tag(_, _, ?tps, ?did, ?ann)) {
            ty_param_count = _vec::len[ast::ty_param](tps);
            result_ty = ann_to_type(ntt, ann);
        }
        case (ast::item_obj(_, _, ?tps, _, ?ann)) {
            ty_param_count = _vec::len[ast::ty_param](tps);
            result_ty = ann_to_type(ntt, ann);
        }
    }

    ret tup(ty_param_count, result_ty);
}

fn stmt_ty(&ctxt cx, &node_type_table ntt, &@ast::stmt s) -> t {
    alt (s.node) {
        case (ast::stmt_expr(?e,_)) {
            ret expr_ty(cx, ntt, e);
        }
        case (_) {
            ret mk_nil(cx);
        }
    }
}

fn block_ty(&ctxt cx, &node_type_table ntt, &ast::block b) -> t {
    alt (b.node.expr) {
        case (some[@ast::expr](?e)) { ret expr_ty(cx, ntt, e); }
        case (none[@ast::expr])     { ret mk_nil(cx); }
    }
}

// Returns the type of a pattern as a monotype. Like @expr_ty, this function
// doesn't provide type parameter substitutions.
fn pat_ty(&ctxt cx, &node_type_table ntt, &@ast::pat pat) -> t {
    alt (pat.node) {
        case (ast::pat_wild(?ann))      { ret ann_to_monotype(cx, ntt, ann); }
        case (ast::pat_lit(_, ?ann))    { ret ann_to_monotype(cx, ntt, ann); }
        case (ast::pat_bind(_, _, ?ann)) {
            ret ann_to_monotype(cx, ntt, ann);
        }
        case (ast::pat_tag(_, _, ?ann)) {
            ret ann_to_monotype(cx, ntt, ann);
        }
    }
    fail;   // not reached
}

fn expr_ann(&@ast::expr e) -> ast::ann {
    alt (e.node) {
        case (ast::expr_vec(_,_,?a)) { ret a; }
        case (ast::expr_tup(_,?a)) { ret a; }
        case (ast::expr_rec(_,_,?a)) { ret a; }
        case (ast::expr_call(_,_,?a)) { ret a; }
        case (ast::expr_bind(_,_,?a)) { ret a; }
        case (ast::expr_binary(_,_,_,?a)) { ret a; }
        case (ast::expr_unary(_,_,?a)) { ret a; }
        case (ast::expr_lit(_,?a)) { ret a; }
        case (ast::expr_cast(_,_,?a)) { ret a; }
        case (ast::expr_if(_,_,_,?a)) { ret a; }
        case (ast::expr_while(_,_,?a)) { ret a; }
        case (ast::expr_for(_,_,_,?a)) { ret a; }
        case (ast::expr_for_each(_,_,_,?a)) { ret a; }
        case (ast::expr_do_while(_,_,?a)) { ret a; }
        case (ast::expr_alt(_,_,?a)) { ret a; }
        case (ast::expr_block(_,?a)) { ret a; }
        case (ast::expr_assign(_,_,?a)) { ret a; }
        case (ast::expr_assign_op(_,_,_,?a)) { ret a; }
        case (ast::expr_send(_,_,?a)) { ret a; }
        case (ast::expr_recv(_,_,?a)) { ret a; }
        case (ast::expr_field(_,_,?a)) { ret a; }
        case (ast::expr_index(_,_,?a)) { ret a; }
        case (ast::expr_path(_,?a)) { ret a; }
        case (ast::expr_ext(_,_,_,_,?a)) { ret a; }
        case (ast::expr_fail(?a)) { ret a; }
        case (ast::expr_ret(_,?a)) { ret a; }
        case (ast::expr_put(_,?a)) { ret a; }
        case (ast::expr_be(_,?a)) { ret a; }
        case (ast::expr_log(_,_,?a)) { ret a; }
        case (ast::expr_assert(_,?a)) { ret a; }
        case (ast::expr_check(_,?a)) { ret a; }
        case (ast::expr_port(?a)) { ret a; }
        case (ast::expr_chan(_,?a)) { ret a; }
        case (ast::expr_anon_obj(_,_,_,?a)) { ret a; }
        case (ast::expr_break(?a)) { ret a; }
        case (ast::expr_cont(?a)) { ret a; }
        case (ast::expr_self_method(_, ?a)) { ret a; }
    }
}

// Returns the type of an expression as a monotype.
//
// NB: This type doesn't provide type parameter substitutions; e.g. if you
// ask for the type of "id" in "id(3)", it will return "fn(&int) -> int"
// instead of "fn(&T) -> T with T = int". If this isn't what you want, see
// expr_ty_params_and_ty() below.
fn expr_ty(&ctxt cx, &node_type_table ntt, &@ast::expr expr) -> t {
    ret ann_to_monotype(cx, ntt, expr_ann(expr));
}

fn expr_ty_params_and_ty(&ctxt cx, &node_type_table ntt, &@ast::expr expr)
        -> tup(vec[t], t) {
    auto a = expr_ann(expr);

    ret tup(ann_to_type_params(ntt, a), ann_to_type(ntt, a));
}

fn expr_has_ty_params(&node_type_table ntt, &@ast::expr expr) -> bool {
    ret ann_has_type_params(ntt, expr_ann(expr));
}


// Expression utilities

fn field_num(&session::session sess, &span sp,
             &ast::ident id) -> uint {
    let uint accum = 0u;
    let uint i = 0u;
    for (u8 c in id) {
        if (i == 0u) {
            if (c != ('_' as u8)) {
                sess.span_err(sp,
                              "bad numeric field on tuple: "
                              + "missing leading underscore");
            }
        } else {
            if (('0' as u8) <= c && c <= ('9' as u8)) {
                accum *= 10u;
                accum += (c as uint) - ('0' as uint);
            } else {
                auto s = "";
                s += _str::unsafe_from_byte(c);
                sess.span_err(sp,
                              "bad numeric field on tuple: "
                              + " non-digit character: "
                              + s);
            }
        }
        i += 1u;
    }
    ret accum;
}

fn field_idx(&session::session sess, &span sp,
             &ast::ident id, &vec[field] fields) -> uint {
    let uint i = 0u;
    for (field f in fields) {
        if (_str::eq(f.ident, id)) {
            ret i;
        }
        i += 1u;
    }
    sess.span_err(sp, "unknown field '" + id + "' of record");
    fail;
}

fn method_idx(&session::session sess, &span sp,
              &ast::ident id, &vec[method] meths) -> uint {
    let uint i = 0u;
    for (method m in meths) {
        if (_str::eq(m.ident, id)) {
            ret i;
        }
        i += 1u;
    }
    sess.span_err(sp, "unknown method '" + id + "' of obj");
    fail;
}

fn sort_methods(&vec[method] meths) -> vec[method] {
    fn method_lteq(&method a, &method b) -> bool {
        ret _str::lteq(a.ident, b.ident);
    }

    ret std::sort::merge_sort[method](bind method_lteq(_,_), meths);
}

fn is_lval(&@ast::expr expr) -> bool {
    alt (expr.node) {
        case (ast::expr_field(_,_,_))    { ret true;  }
        case (ast::expr_index(_,_,_))    { ret true;  }
        case (ast::expr_path(_,_))       { ret true;  }
        case (ast::expr_unary(ast::deref,_,_))  { ret true; }
        case (_)                        { ret false; }
    }
}

// Type unification via Robinson's algorithm (Robinson 1965). Implemented as
// described in Hoder and Voronkov:
//
//     http://www.cs.man.ac.uk/~hoderk/ubench/unification_full.pdf

mod Unify {
    tag result {
        ures_ok(t);
        ures_err(type_err, t, t);
    }

    type ctxt = rec(ufind::ufind sets,
                    hashmap[int,uint] var_ids,
                    mutable vec[mutable vec[t]] types,
                    unify_handler handler,
                    ty_ctxt tcx);

    // Wraps the given type in an appropriate cname.
    //
    // TODO: This doesn't do anything yet. We should carry the cname up from
    // the expected and/or actual types when unification results in a type
    // identical to one or both of the two. The precise algorithm for this is
    // something we'll probably need to develop over time.

    // Simple structural type comparison.
    fn struct_cmp(@ctxt cx, t expected, t actual) -> result {
        if (struct(cx.tcx, expected) == struct(cx.tcx, actual)) {
            ret ures_ok(expected);
        }

        ret ures_err(terr_mismatch, expected, actual);
    }

    // Unifies two mutability flags.
    fn unify_mut(ast::mutability expected, ast::mutability actual)
            -> option::t[ast::mutability] {
        if (expected == actual) {
            ret some[ast::mutability](expected);
        }
        if (expected == ast::maybe_mut) {
            ret some[ast::mutability](actual);
        }
        if (actual == ast::maybe_mut) {
            ret some[ast::mutability](expected);
        }
        ret none[ast::mutability];
    }

    tag fn_common_res {
        fn_common_res_err(result);
        fn_common_res_ok(vec[arg], t);
    }

    fn unify_fn_common(&@ctxt cx,
                       &t expected,
                       &t actual,
                       &vec[arg] expected_inputs, &t expected_output,
                       &vec[arg] actual_inputs, &t actual_output)
        -> fn_common_res {
        auto expected_len = _vec::len[arg](expected_inputs);
        auto actual_len = _vec::len[arg](actual_inputs);
        if (expected_len != actual_len) {
            ret fn_common_res_err(ures_err(terr_arg_count,
                                           expected, actual));
        }

        // TODO: as above, we should have an iter2 iterator.
        let vec[arg] result_ins = vec();
        auto i = 0u;
        while (i < expected_len) {
            auto expected_input = expected_inputs.(i);
            auto actual_input = actual_inputs.(i);

            // Unify the result modes. "mo_either" unifies with both modes.
            auto result_mode;
            if (expected_input.mode == mo_either) {
                result_mode = actual_input.mode;
            } else if (actual_input.mode == mo_either) {
                result_mode = expected_input.mode;
            } else if (expected_input.mode != actual_input.mode) {
                // FIXME this is the wrong error
                ret fn_common_res_err(ures_err(terr_arg_count,
                                               expected, actual));
            } else {
                result_mode = expected_input.mode;
            }

            auto result = unify_step(cx, actual_input.ty, expected_input.ty);

            alt (result) {
                case (ures_ok(?rty)) {
                    result_ins += vec(rec(mode=result_mode, ty=rty));
                }

                case (_) {
                    ret fn_common_res_err(result);
                }
            }

            i += 1u;
        }

        // Check the output.
        auto result = unify_step(cx, expected_output, actual_output);
        alt (result) {
            case (ures_ok(?rty)) {
                ret fn_common_res_ok(result_ins, rty);
            }

            case (_) {
                ret fn_common_res_err(result);
            }
        }
    }

    fn unify_fn(&@ctxt cx,
                &ast::proto e_proto,
                &ast::proto a_proto,
                &t expected,
                &t actual,
                &vec[arg] expected_inputs, &t expected_output,
                &vec[arg] actual_inputs, &t actual_output)
        -> result {

        if (e_proto != a_proto) {
            ret ures_err(terr_mismatch, expected, actual);
        }
        auto t = unify_fn_common(cx, expected, actual,
                                 expected_inputs, expected_output,
                                 actual_inputs, actual_output);
        alt (t) {
            case (fn_common_res_err(?r)) {
                ret r;
            }
            case (fn_common_res_ok(?result_ins, ?result_out)) {
                auto t2 = mk_fn(cx.tcx, e_proto, result_ins, result_out);
                ret ures_ok(t2);
            }
        }
    }

    fn unify_native_fn(&@ctxt cx,
                       &ast::native_abi e_abi,
                       &ast::native_abi a_abi,
                       &t expected,
                       &t actual,
                       &vec[arg] expected_inputs, &t expected_output,
                       &vec[arg] actual_inputs, &t actual_output)
        -> result {
        if (e_abi != a_abi) {
            ret ures_err(terr_mismatch, expected, actual);
        }

        auto t = unify_fn_common(cx, expected, actual,
                                 expected_inputs, expected_output,
                                 actual_inputs, actual_output);
        alt (t) {
            case (fn_common_res_err(?r)) {
                ret r;
            }
            case (fn_common_res_ok(?result_ins, ?result_out)) {
                auto t2 = mk_native_fn(cx.tcx, e_abi, result_ins,
                                       result_out);
                ret ures_ok(t2);
            }
        }
    }

    fn unify_obj(&@ctxt cx,
                 &t expected,
                 &t actual,
                 &vec[method] expected_meths,
                 &vec[method] actual_meths) -> result {
      let vec[method] result_meths = vec();
      let uint i = 0u;
      let uint expected_len = _vec::len[method](expected_meths);
      let uint actual_len = _vec::len[method](actual_meths);

      if (expected_len != actual_len) {
        ret ures_err(terr_meth_count, expected, actual);
      }

      while (i < expected_len) {
        auto e_meth = expected_meths.(i);
        auto a_meth = actual_meths.(i);
        if (! _str::eq(e_meth.ident, a_meth.ident)) {
          ret ures_err(terr_obj_meths(e_meth.ident, a_meth.ident),
                       expected, actual);
        }
        auto r = unify_fn(cx,
                          e_meth.proto, a_meth.proto,
                          expected, actual,
                          e_meth.inputs, e_meth.output,
                          a_meth.inputs, a_meth.output);
        alt (r) {
            case (ures_ok(?tfn)) {
                alt (struct(cx.tcx, tfn)) {
                    case (ty_fn(?proto, ?ins, ?out)) {
                        result_meths += vec(rec(inputs = ins,
                                                output = out
                                                with e_meth));
                    }
                }
            }
            case (_) {
                ret r;
            }
        }
        i += 1u;
      }
      auto t = mk_obj(cx.tcx, result_meths);
      ret ures_ok(t);
    }

    fn get_or_create_set(&@ctxt cx, int id) -> uint {
        auto set_num;
        alt (cx.var_ids.find(id)) {
        case (none[uint]) {
            set_num = ufind::make_set(cx.sets);
            cx.var_ids.insert(id, set_num);
        }
        case (some[uint](?n)) { set_num = n; }
        }
        ret set_num;
    }

    fn unify_step(&@ctxt cx, &t expected, &t actual) -> result {
        // TODO: rewrite this using tuple pattern matching when available, to
        // avoid all this rightward drift and spikiness.

        // TODO: occurs check, to make sure we don't loop forever when
        // unifying e.g. 'a and option['a]

        // Fast path.
        if (eq_ty(expected, actual)) { ret ures_ok(expected); }

        alt (struct(cx.tcx, actual)) {
            // If the RHS is a variable type, then just do the appropriate
            // binding.
            case (ty::ty_var(?actual_id)) {
                auto actual_n = get_or_create_set(cx, actual_id);
                alt (struct(cx.tcx, expected)) {
                    case (ty::ty_var(?expected_id)) {
                        auto expected_n = get_or_create_set(cx, expected_id);
                        ufind::union(cx.sets, expected_n, actual_n);
                    }

                    case (_) {
                        // Just bind the type variable to the expected type.
                        auto vlen = _vec::len[vec[t]](cx.types);
                        if (actual_n < vlen) {
                            cx.types.(actual_n) += vec(expected);
                        } else {
                            assert (actual_n == vlen);
                            cx.types += vec(mutable vec(expected));
                        }
                    }
                }
                ret ures_ok(actual);
            }
            case (ty::ty_local(?actual_id)) {
                auto result_ty;
                alt (cx.handler.resolve_local(actual_id)) {
                    case (none[t]) { result_ty = expected; }
                    case (some[t](?actual_ty)) {
                        auto result = unify_step(cx, expected, actual_ty);
                        alt (result) {
                            case (ures_ok(?rty)) { result_ty = rty; }
                            case (_) { ret result; }
                        }
                    }
                }

                cx.handler.record_local(actual_id, result_ty);
                ret ures_ok(result_ty);
            }
            case (ty::ty_bound_param(?actual_id)) {
                alt (struct(cx.tcx, expected)) {
                    case (ty::ty_local(_)) {
                        log_err "TODO: bound param unifying with local";
                        fail;
                    }

                    case (_) {
                        ret cx.handler.record_param(actual_id, expected);
                    }
                }
            }
            case (_) { /* empty */ }
        }

        alt (struct(cx.tcx, expected)) {
            case (ty::ty_nil)        { ret struct_cmp(cx, expected, actual); }
            case (ty::ty_bool)       { ret struct_cmp(cx, expected, actual); }
            case (ty::ty_int)        { ret struct_cmp(cx, expected, actual); }
            case (ty::ty_uint)       { ret struct_cmp(cx, expected, actual); }
            case (ty::ty_machine(_)) { ret struct_cmp(cx, expected, actual); }
            case (ty::ty_float)      { ret struct_cmp(cx, expected, actual); }
            case (ty::ty_char)       { ret struct_cmp(cx, expected, actual); }
            case (ty::ty_str)        { ret struct_cmp(cx, expected, actual); }
            case (ty::ty_type)       { ret struct_cmp(cx, expected, actual); }
            case (ty::ty_native)     { ret struct_cmp(cx, expected, actual); }
            case (ty::ty_param(_))   { ret struct_cmp(cx, expected, actual); }

            case (ty::ty_tag(?expected_id, ?expected_tps)) {
                alt (struct(cx.tcx, actual)) {
                    case (ty::ty_tag(?actual_id, ?actual_tps)) {
                        if (expected_id._0 != actual_id._0 ||
                                expected_id._1 != actual_id._1) {
                            ret ures_err(terr_mismatch, expected, actual);
                        }

                        // TODO: factor this cruft out, see the TODO in the
                        // ty::ty_tup case
                        let vec[t] result_tps = vec();
                        auto i = 0u;
                        auto expected_len = _vec::len[t](expected_tps);
                        while (i < expected_len) {
                            auto expected_tp = expected_tps.(i);
                            auto actual_tp = actual_tps.(i);

                            auto result = unify_step(cx,
                                                     expected_tp,
                                                     actual_tp);

                            alt (result) {
                                case (ures_ok(?rty)) {
                                    _vec::push[t](result_tps, rty);
                                }
                                case (_) {
                                    ret result;
                                }
                            }

                            i += 1u;
                        }

                        ret ures_ok(mk_tag(cx.tcx, expected_id, result_tps));
                    }
                    case (_) { /* fall through */ }
                }

                ret ures_err(terr_mismatch, expected, actual);
            }

            case (ty::ty_box(?expected_mt)) {
                alt (struct(cx.tcx, actual)) {
                    case (ty::ty_box(?actual_mt)) {
                        auto mut;
                        alt (unify_mut(expected_mt.mut, actual_mt.mut)) {
                            case (none[ast::mutability]) {
                                ret ures_err(terr_box_mutability, expected,
                                             actual);
                            }
                            case (some[ast::mutability](?m)) { mut = m; }
                        }

                        auto result = unify_step(cx,
                                                 expected_mt.ty,
                                                 actual_mt.ty);
                        alt (result) {
                            case (ures_ok(?result_sub)) {
                                auto mt = rec(ty=result_sub, mut=mut);
                                ret ures_ok(mk_box(cx.tcx, mt));
                            }
                            case (_) {
                                ret result;
                            }
                        }
                    }

                    case (_) {
                        ret ures_err(terr_mismatch, expected, actual);
                    }
                }
            }

            case (ty::ty_vec(?expected_mt)) {
                alt (struct(cx.tcx, actual)) {
                    case (ty::ty_vec(?actual_mt)) {
                        auto mut;
                        alt (unify_mut(expected_mt.mut, actual_mt.mut)) {
                            case (none[ast::mutability]) {
                                ret ures_err(terr_vec_mutability, expected,
                                             actual);
                            }
                            case (some[ast::mutability](?m)) { mut = m; }
                        }

                        auto result = unify_step(cx,
                                                 expected_mt.ty,
                                                 actual_mt.ty);
                        alt (result) {
                            case (ures_ok(?result_sub)) {
                                auto mt = rec(ty=result_sub, mut=mut);
                                ret ures_ok(mk_vec(cx.tcx, mt));
                            }
                            case (_) {
                                ret result;
                            }
                        }
                    }

                    case (_) {
                        ret ures_err(terr_mismatch, expected, actual);
                   }
                }
            }

            case (ty::ty_port(?expected_sub)) {
                alt (struct(cx.tcx, actual)) {
                    case (ty::ty_port(?actual_sub)) {
                        auto result = unify_step(cx,
                                                 expected_sub,
                                                 actual_sub);
                        alt (result) {
                            case (ures_ok(?result_sub)) {
                                ret ures_ok(mk_port(cx.tcx, result_sub));
                            }
                            case (_) {
                                ret result;
                            }
                        }
                    }

                    case (_) {
                        ret ures_err(terr_mismatch, expected, actual);
                    }
                }
            }

            case (ty::ty_chan(?expected_sub)) {
                alt (struct(cx.tcx, actual)) {
                    case (ty::ty_chan(?actual_sub)) {
                        auto result = unify_step(cx,
                                                 expected_sub,
                                                 actual_sub);
                        alt (result) {
                            case (ures_ok(?result_sub)) {
                                ret ures_ok(mk_chan(cx.tcx, result_sub));
                            }
                            case (_) {
                                ret result;
                            }
                        }
                    }

                    case (_) {
                        ret ures_err(terr_mismatch, expected, actual);
                    }
                }
            }

            case (ty::ty_tup(?expected_elems)) {
                alt (struct(cx.tcx, actual)) {
                    case (ty::ty_tup(?actual_elems)) {
                        auto expected_len = _vec::len[ty::mt](expected_elems);
                        auto actual_len = _vec::len[ty::mt](actual_elems);
                        if (expected_len != actual_len) {
                            auto err = terr_tuple_size(expected_len,
                                                       actual_len);
                            ret ures_err(err, expected, actual);
                        }

                        // TODO: implement an iterator that can iterate over
                        // two arrays simultaneously.
                        let vec[ty::mt] result_elems = vec();
                        auto i = 0u;
                        while (i < expected_len) {
                            auto expected_elem = expected_elems.(i);
                            auto actual_elem = actual_elems.(i);

                            auto mut;
                            alt (unify_mut(expected_elem.mut,
                                           actual_elem.mut)) {
                                case (none[ast::mutability]) {
                                    auto err = terr_tuple_mutability;
                                    ret ures_err(err, expected, actual);
                                }
                                case (some[ast::mutability](?m)) { mut = m; }
                            }

                            auto result = unify_step(cx,
                                                     expected_elem.ty,
                                                     actual_elem.ty);
                            alt (result) {
                                case (ures_ok(?rty)) {
                                    auto mt = rec(ty=rty, mut=mut);
                                    result_elems += vec(mt);
                                }
                                case (_) {
                                    ret result;
                                }
                            }

                            i += 1u;
                        }

                        ret ures_ok(mk_tup(cx.tcx, result_elems));
                    }

                    case (_) {
                        ret ures_err(terr_mismatch, expected, actual);
                    }
                }
            }

            case (ty::ty_rec(?expected_fields)) {
                alt (struct(cx.tcx, actual)) {
                    case (ty::ty_rec(?actual_fields)) {
                        auto expected_len = _vec::len[field](expected_fields);
                        auto actual_len = _vec::len[field](actual_fields);
                        if (expected_len != actual_len) {
                            auto err = terr_record_size(expected_len,
                                                        actual_len);
                            ret ures_err(err, expected, actual);
                        }

                        // TODO: implement an iterator that can iterate over
                        // two arrays simultaneously.
                        let vec[field] result_fields = vec();
                        auto i = 0u;
                        while (i < expected_len) {
                            auto expected_field = expected_fields.(i);
                            auto actual_field = actual_fields.(i);

                            auto mut;
                            alt (unify_mut(expected_field.mt.mut,
                                           actual_field.mt.mut)) {
                                case (none[ast::mutability]) {
                                    ret ures_err(terr_record_mutability,
                                                 expected, actual);
                                }
                                case (some[ast::mutability](?m)) { mut = m; }
                            }

                            if (!_str::eq(expected_field.ident,
                                         actual_field.ident)) {
                                auto err =
                                    terr_record_fields(expected_field.ident,
                                                       actual_field.ident);
                                ret ures_err(err, expected, actual);
                            }

                            auto result = unify_step(cx,
                                                     expected_field.mt.ty,
                                                     actual_field.mt.ty);
                            alt (result) {
                                case (ures_ok(?rty)) {
                                    auto mt = rec(ty=rty, mut=mut);
                                    _vec::push[field]
                                        (result_fields,
                                         rec(mt=mt with expected_field));
                                }
                                case (_) {
                                    ret result;
                                }
                            }

                            i += 1u;
                        }

                        ret ures_ok(mk_rec(cx.tcx, result_fields));
                    }

                    case (_) {
                        ret ures_err(terr_mismatch, expected, actual);
                    }
                }
            }

            case (ty::ty_fn(?ep, ?expected_inputs, ?expected_output)) {
                alt (struct(cx.tcx, actual)) {
                    case (ty::ty_fn(?ap, ?actual_inputs, ?actual_output)) {
                        ret unify_fn(cx, ep, ap,
                                     expected, actual,
                                     expected_inputs, expected_output,
                                     actual_inputs, actual_output);
                    }

                    case (_) {
                        ret ures_err(terr_mismatch, expected, actual);
                    }
                }
            }

            case (ty::ty_native_fn(?e_abi, ?expected_inputs,
                                  ?expected_output)) {
                alt (struct(cx.tcx, actual)) {
                    case (ty::ty_native_fn(?a_abi, ?actual_inputs,
                                          ?actual_output)) {
                        ret unify_native_fn(cx, e_abi, a_abi,
                                            expected, actual,
                                            expected_inputs, expected_output,
                                            actual_inputs, actual_output);
                    }
                    case (_) {
                        ret ures_err(terr_mismatch, expected, actual);
                    }
                }
            }

            case (ty::ty_obj(?expected_meths)) {
                alt (struct(cx.tcx, actual)) {
                    case (ty::ty_obj(?actual_meths)) {
                        ret unify_obj(cx, expected, actual,
                                      expected_meths, actual_meths);
                    }
                    case (_) {
                        ret ures_err(terr_mismatch, expected, actual);
                    }
                }
            }

            case (ty::ty_var(?expected_id)) {
                // Add a binding.
                auto expected_n = get_or_create_set(cx, expected_id);
                auto vlen = _vec::len[vec[t]](cx.types);
                if (expected_n < vlen) {
                    cx.types.(expected_n) += vec(actual);
                } else {
                    assert (expected_n == vlen);
                    cx.types += vec(mutable vec(actual));
                }
                ret ures_ok(expected);
            }

            case (ty::ty_local(?expected_id)) {
                auto result_ty;
                alt (cx.handler.resolve_local(expected_id)) {
                    case (none[t]) { result_ty = actual; }
                    case (some[t](?expected_ty)) {
                        auto result = unify_step(cx, expected_ty, actual);
                        alt (result) {
                            case (ures_ok(?rty)) { result_ty = rty; }
                            case (_) { ret result; }
                        }
                    }
                }

                cx.handler.record_local(expected_id, result_ty);
                ret ures_ok(result_ty);
            }

            case (ty::ty_bound_param(?expected_id)) {
                ret cx.handler.record_param(expected_id, actual);
            }
        }

        // TODO: remove me once match-exhaustiveness checking works
        fail;
    }

    // Performs type binding substitution.
    fn substitute(&@ctxt cx, &vec[t] set_types, &t typ) -> t {
        if (!type_contains_vars(cx.tcx, typ)) {
            ret typ;
        }

        fn substituter(@ctxt cx, vec[t] types, t typ) -> t {
            alt (struct(cx.tcx, typ)) {
                case (ty_var(?id)) {
                    alt (cx.var_ids.find(id)) {
                        case (some[uint](?n)) {
                            auto root = ufind::find(cx.sets, n);
                            ret types.(root);
                        }
                        case (none[uint]) { ret typ; }
                    }
                }
                case (_) { ret typ; }
            }
        }

        auto f = bind substituter(cx, set_types, _);
        ret fold_ty(cx.tcx, f, typ);
    }

    fn unify_sets(&@ctxt cx) -> vec[t] {
        let vec[t] throwaway = vec();
        let vec[mutable vec[t]] set_types = vec(mutable throwaway);
        _vec::pop[vec[t]](set_types);   // FIXME: botch

        for (ufind::node node in cx.sets.nodes) {
            let vec[t] v = vec();
            set_types += vec(mutable v);
        }

        auto i = 0u;
        while (i < _vec::len[vec[t]](set_types)) {
            auto root = ufind::find(cx.sets, i);
            set_types.(root) += cx.types.(i);
            i += 1u;
        }

        let vec[t] result = vec();
        for (vec[t] types in set_types) {
            if (_vec::len[t](types) > 1u) {
                log_err "unification of > 1 types in a type set is " +
                    "unimplemented";
                fail;
            }
            result += vec(types.(0));
        }

        ret result;
    }

    fn unify(&t expected,
             &t actual,
             &unify_handler handler,
             &ty_ctxt tcx) -> result {
        let vec[t] throwaway = vec();
        let vec[mutable vec[t]] types = vec(mutable throwaway);
        _vec::pop[vec[t]](types);   // FIXME: botch

        auto cx = @rec(sets=ufind::make(),
                       var_ids=common::new_int_hash[uint](),
                       mutable types=types,
                       handler=handler,
                       tcx=tcx);

        auto ures = unify_step(cx, expected, actual);
        alt (ures) {
        case (ures_ok(?typ)) {
            // Fast path: if there are no local variables, don't perform
            // substitutions.
            if (_vec::len(cx.sets.nodes) == 0u) {
                ret ures_ok(typ);
            }

            auto set_types = unify_sets(cx);
            auto t2 = substitute(cx, set_types, typ);
            ret ures_ok(t2);
        }
        case (_) { ret ures; }
        }
        fail;   // not reached
    }
}

fn type_err_to_str(&ty::type_err err) -> str {
    alt (err) {
        case (terr_mismatch) {
            ret "types differ";
        }
        case (terr_box_mutability) {
            ret "boxed values differ in mutability";
        }
        case (terr_vec_mutability) {
            ret "vectors differ in mutability";
        }
        case (terr_tuple_size(?e_sz, ?a_sz)) {
            ret "expected a tuple with " + _uint::to_str(e_sz, 10u) +
                " elements but found one with " + _uint::to_str(a_sz, 10u) +
                " elements";
        }
        case (terr_tuple_mutability) {
            ret "tuple elements differ in mutability";
        }
        case (terr_record_size(?e_sz, ?a_sz)) {
            ret "expected a record with " + _uint::to_str(e_sz, 10u) +
                " fields but found one with " + _uint::to_str(a_sz, 10u) +
                " fields";
        }
        case (terr_record_mutability) {
            ret "record elements differ in mutability";
        }
        case (terr_record_fields(?e_fld, ?a_fld)) {
            ret "expected a record with field '" + e_fld +
                "' but found one with field '" + a_fld +
                "'";
        }
        case (terr_arg_count) {
            ret "incorrect number of function parameters";
        }
        case (terr_meth_count) {
            ret "incorrect number of object methods";
        }
        case (terr_obj_meths(?e_meth, ?a_meth)) {
            ret "expected an obj with method '" + e_meth +
                "' but found one with method '" + a_meth +
                "'";
        }
    }
}

// Performs bound type parameter replacement using the supplied mapping from
// parameter IDs to types.
fn substitute_type_params(&ctxt cx, &vec[t] bindings, &t typ) -> t {
    if (!type_contains_bound_params(cx, typ)) {
        ret typ;
    }
    fn replacer(ctxt cx, vec[t] bindings, t typ) -> t {
        alt (struct(cx, typ)) {
            case (ty_bound_param(?param_index)) {
                ret bindings.(param_index);
            }
            case (_) { ret typ; }
        }
    }

    auto f = bind replacer(cx, bindings, _);
    ret fold_ty(cx, f, typ);
}

// Converts type parameters in a type to bound type parameters.
fn bind_params_in_type(&ctxt cx, &t typ) -> t {
    if (!type_contains_params(cx, typ)) {
        ret typ;
    }
    fn binder(ctxt cx, t typ) -> t {
        alt (struct(cx, typ)) {
            case (ty_bound_param(?index)) {
                log_err "bind_params_in_type() called on type that already " +
                    "has bound params in it";
                fail;
            }
            case (ty_param(?index)) { ret mk_bound_param(cx, index); }
            case (_) { ret typ; }
        }
    }

    auto f = bind binder(cx, _);
    ret fold_ty(cx, f, typ);
}


fn def_has_ty_params(&ast::def def) -> bool {
    alt (def) {
        case (ast::def_fn(_))            { ret true;  }
        case (ast::def_obj(_))           { ret true;  }
        case (ast::def_obj_field(_))     { ret false; }
        case (ast::def_mod(_))           { ret false; }
        case (ast::def_const(_))         { ret false; }
        case (ast::def_arg(_))           { ret false; }
        case (ast::def_local(_))         { ret false; }
        case (ast::def_variant(_, _))    { ret true;  }
        case (ast::def_ty(_))            { ret false; }
        case (ast::def_ty_arg(_))        { ret false; }
        case (ast::def_binding(_))       { ret false; }
        case (ast::def_use(_))           { ret false; }
        case (ast::def_native_ty(_))     { ret false; }
        case (ast::def_native_fn(_))     { ret true;  }
    }
}

// If the given item is in an external crate, looks up its type and adds it to
// the type cache. Returns the type parameters and type.
fn lookup_item_type(session::session sess,
                    ctxt cx,
                    &type_cache cache,
                    ast::def_id did) -> ty_param_count_and_ty {
    if (did._0 == sess.get_targ_crate_num()) {
        // The item is in this crate. The caller should have added it to the
        // type cache already; we simply return it.
        ret cache.get(did);
    }

    alt (cache.find(did)) {
        case (some[ty_param_count_and_ty](?tpt)) { ret tpt; }
        case (none[ty_param_count_and_ty]) {
            auto tyt = creader::get_type(sess, cx, did);
            cache.insert(did, tyt);
            ret tyt;
        }
    }
}


// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C $RBUILD 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
