import std._str;
import std._uint;
import std._vec;
import std.map;
import std.map.hashmap;
import std.option;
import std.option.none;
import std.option.some;

import driver.session;
import front.ast;
import front.ast.mutability;
import util.common;
import util.common.append;
import util.common.new_def_hash;
import util.common.span;

// Data types

type arg = rec(ast.mode mode, @t ty);
type field = rec(ast.ident ident, @t ty);
type method = rec(ast.proto proto,
                  ast.ident ident,
                  vec[arg] inputs,
                  @t output);

// NB: If you change this, you'll probably want to change the corresponding
// AST structure in front/ast.rs as well.
type t = rec(sty struct, mutability mut, option.t[str] cname);
tag sty {
    ty_nil;
    ty_bool;
    ty_int;
    ty_uint;
    ty_machine(util.common.ty_mach);
    ty_char;
    ty_str;
    ty_tag(ast.def_id, vec[@t]);
    ty_box(@t);
    ty_vec(@t);
    ty_tup(vec[@t]);
    ty_rec(vec[field]);
    ty_fn(ast.proto, vec[arg], @t);                 // TODO: effect
    ty_native_fn(ast.native_abi, vec[arg], @t);     // TODO: effect
    ty_obj(vec[method]);
    ty_var(int);                                    // ephemeral type var
    ty_local(ast.def_id);                           // type of a local var
    ty_param(ast.def_id);                           // fn/tag type param
    ty_type;
    ty_native;
    // TODO: ty_fn_arg(@t), for a possibly-aliased function argument
}

// Data structures used in type unification

type unify_handler = obj {
    fn resolve_local(ast.def_id id) -> @t;
    fn record_local(ast.def_id id, @t ty);
    fn unify_expected_param(ast.def_id id, @t expected, @t actual)
        -> unify_result;
    fn unify_actual_param(ast.def_id id, @t expected, @t actual)
        -> unify_result;
};

tag type_err {
    terr_mismatch;
    terr_tuple_size(uint, uint);
    terr_tuple_mutability;
    terr_record_size(uint, uint);
    terr_record_mutability;
    terr_record_fields(ast.ident,ast.ident);
    terr_meth_count;
    terr_obj_meths(ast.ident,ast.ident);
    terr_arg_count;
}

tag unify_result {
    ures_ok(@ty.t);
    ures_err(type_err, @ty.t, @ty.t);
}

// Stringification

fn ast_ty_to_str(&@ast.ty ty) -> str {

    fn ast_fn_input_to_str(&rec(ast.mode mode, @ast.ty ty) input) -> str {
        auto s;
        if (mode_is_alias(input.mode)) {
            s = "&";
        } else {
            s = "";
        }

        ret s + ast_ty_to_str(input.ty);
    }

    fn ast_ty_field_to_str(&ast.ty_field f) -> str {
        ret ast_ty_to_str(f.ty) + " " + f.ident;
    }

    auto s;
    alt (ty.node) {
        case (ast.ty_nil)          { s = "()";                            }
        case (ast.ty_bool)         { s = "bool";                          }
        case (ast.ty_int)          { s = "int";                           }
        case (ast.ty_uint)         { s = "uint";                          }
        case (ast.ty_machine(?tm)) { s = common.ty_mach_to_str(tm);       }
        case (ast.ty_char)         { s = "char";                          }
        case (ast.ty_str)          { s = "str";                           }
        case (ast.ty_box(?t))      { s = "@" + ast_ty_to_str(t);          }
        case (ast.ty_vec(?t))      { s = "vec[" + ast_ty_to_str(t) + "]"; }
        case (ast.ty_type)         { s = "type";                          }

        case (ast.ty_tup(?elts)) {
            auto f = ast_ty_to_str;
            s = "tup(";
            s += _str.connect(_vec.map[@ast.ty,str](f, elts), ",");
            s += ")";
        }

        case (ast.ty_rec(?fields)) {
            auto f = ast_ty_field_to_str;
            s = "rec(";
            s += _str.connect(_vec.map[ast.ty_field,str](f, fields), ",");
            s += ")";
        }

        case (ast.ty_fn(?proto, ?inputs, ?output)) {
            auto f = ast_fn_input_to_str;
            if (proto == ast.proto_fn) {
                s = "fn(";
            } else {
                s = "iter(";
            }
            auto is = _vec.map[rec(ast.mode mode, @ast.ty ty),str](f, inputs);
            s += _str.connect(is, ", ");
            s += ")";

            if (output.node != ast.ty_nil) {
                s += " -> " + ast_ty_to_str(output);
            }
        }

        case (ast.ty_path(?path, _)) {
            s = path_to_str(path);
        }

        case (ast.ty_mutable(?t)) {
            s = "mutable " + ast_ty_to_str(t);
        }


        case (_) {
            fail;   // FIXME: typestate bug
        }
    }

    ret s;
}

fn path_to_str(&ast.path pth) -> str {
    auto result = _str.connect(pth.node.idents,  ".");
    if (_vec.len[@ast.ty](pth.node.types) > 0u) {
        auto f = ast_ty_to_str;
        result += "[";
        result += _str.connect(_vec.map[@ast.ty,str](f, pth.node.types), ",");
        result += "]";
    }
    ret result;
}

fn ty_to_str(&@t typ) -> str {

    fn fn_input_to_str(&rec(ast.mode mode, @t ty) input) -> str {
        auto s;
        if (mode_is_alias(input.mode)) {
            s = "&";
        } else {
            s = "";
        }

        ret s + ty_to_str(input.ty);
    }

    fn fn_to_str(ast.proto proto,
                 option.t[ast.ident] ident,
                 vec[arg] inputs, @t output) -> str {
            auto f = fn_input_to_str;
            auto s = "fn";
            if (proto == ast.proto_iter) {
                s = "iter";
            }
            alt (ident) {
                case (some[ast.ident](?i)) {
                    s += " ";
                    s += i;
                }
                case (_) { }
            }

            s += "(";
            s += _str.connect(_vec.map[arg,str](f, inputs), ", ");
            s += ")";

            if (output.struct != ty_nil) {
                s += " -> " + ty_to_str(output);
            }
            ret s;
    }

    fn method_to_str(&method m) -> str {
        ret fn_to_str(m.proto, some[ast.ident](m.ident),
                      m.inputs, m.output) + ";";
    }

    fn field_to_str(&field f) -> str {
        ret ty_to_str(f.ty) + " " + f.ident;
    }

    auto s = "";
    if (typ.mut == ast.mut) {
        s += "mutable ";
    }

    alt (typ.struct) {
        case (ty_native)       { s = "native";                    }
        case (ty_nil)          { s = "()";                        }
        case (ty_bool)         { s = "bool";                      }
        case (ty_int)          { s = "int";                       }
        case (ty_uint)         { s = "uint";                      }
        case (ty_machine(?tm)) { s = common.ty_mach_to_str(tm);   }
        case (ty_char)         { s = "char";                      }
        case (ty_str)          { s = "str";                       }
        case (ty_box(?t))      { s = "@" + ty_to_str(t);          }
        case (ty_vec(?t))      { s = "vec[" + ty_to_str(t) + "]"; }
        case (ty_type)         { s = "type";                      }

        case (ty_tup(?elems)) {
            auto f = ty_to_str;
            auto strs = _vec.map[@t,str](f, elems);
            s = "tup(" + _str.connect(strs, ",") + ")";
        }

        case (ty_rec(?elems)) {
            auto f = field_to_str;
            auto strs = _vec.map[field,str](f, elems);
            s = "rec(" + _str.connect(strs, ",") + ")";
        }

        case (ty_tag(?id, ?tps)) {
            // The user should never see this if the cname is set properly!
            s = "<tag#" + util.common.istr(id._0) + ":" +
                util.common.istr(id._1) + ">";
            if (_vec.len[@t](tps) > 0u) {
                auto f = ty_to_str;
                auto strs = _vec.map[@t,str](f, tps);
                s += "[" + _str.connect(strs, ",") + "]";
            }
        }

        case (ty_fn(?proto, ?inputs, ?output)) {
            s = fn_to_str(proto, none[ast.ident], inputs, output);
        }

        case (ty_native_fn(_, ?inputs, ?output)) {
            s = fn_to_str(ast.proto_fn, none[ast.ident], inputs, output);
        }

        case (ty_obj(?meths)) {
            auto f = method_to_str;
            auto m = _vec.map[method,str](f, meths);
            s = "obj {\n\t" + _str.connect(m, "\n\t") + "\n}";
        }

        case (ty_var(?v)) {
            s = "<T" + util.common.istr(v) + ">";
        }

        case (ty_local(?id)) {
            s = "<L" + util.common.istr(id._0) + ":" + util.common.istr(id._1)
                + ">";
        }

        case (ty_param(?id)) {
            s = "<P" + util.common.istr(id._0) + ":" + util.common.istr(id._1)
                + ">";
        }
    }

    ret s;
}

// Type folds

type ty_fold = state obj {
    fn fold_simple_ty(@t ty) -> @t;
};

fn fold_ty(ty_fold fld, @t ty) -> @t {
    fn rewrap(@t orig, &sty new) -> @t {
        ret @rec(struct=new, mut=orig.mut, cname=orig.cname);
    }

    alt (ty.struct) {
        case (ty_nil)           { ret fld.fold_simple_ty(ty); }
        case (ty_bool)          { ret fld.fold_simple_ty(ty); }
        case (ty_int)           { ret fld.fold_simple_ty(ty); }
        case (ty_uint)          { ret fld.fold_simple_ty(ty); }
        case (ty_machine(_))    { ret fld.fold_simple_ty(ty); }
        case (ty_char)          { ret fld.fold_simple_ty(ty); }
        case (ty_str)           { ret fld.fold_simple_ty(ty); }
        case (ty_type)          { ret fld.fold_simple_ty(ty); }
        case (ty_native)        { ret fld.fold_simple_ty(ty); }
        case (ty_box(?subty)) {
            ret rewrap(ty, ty_box(fold_ty(fld, subty)));
        }
        case (ty_vec(?subty)) {
            ret rewrap(ty, ty_vec(fold_ty(fld, subty)));
        }
        case (ty_tag(?tid, ?subtys)) {
            let vec[@t] new_subtys = vec();
            for (@t subty in subtys) {
                new_subtys += vec(fold_ty(fld, subty));
            }
            ret rewrap(ty, ty_tag(tid, new_subtys));
        }
        case (ty_tup(?subtys)) {
            let vec[@t] new_subtys = vec();
            for (@t subty in subtys) {
                new_subtys += vec(fold_ty(fld, subty));
            }
            ret rewrap(ty, ty_tup(new_subtys));
        }
        case (ty_rec(?fields)) {
            let vec[field] new_fields = vec();
            for (field fl in fields) {
                auto new_ty = fold_ty(fld, fl.ty);
                new_fields += vec(rec(ident=fl.ident, ty=new_ty));
            }
            ret rewrap(ty, ty_rec(new_fields));
        }
        case (ty_fn(?proto, ?args, ?ret_ty)) {
            let vec[arg] new_args = vec();
            for (arg a in args) {
                auto new_ty = fold_ty(fld, a.ty);
                new_args += vec(rec(mode=a.mode, ty=new_ty));
            }
            ret rewrap(ty, ty_fn(proto, new_args, fold_ty(fld, ret_ty)));
        }
        case (ty_native_fn(?abi, ?args, ?ret_ty)) {
            let vec[arg] new_args = vec();
            for (arg a in args) {
                auto new_ty = fold_ty(fld, a.ty);
                new_args += vec(rec(mode=a.mode, ty=new_ty));
            }
            ret rewrap(ty, ty_native_fn(abi, new_args, fold_ty(fld, ret_ty)));
        }
        case (ty_obj(?methods)) {
            let vec[method] new_methods = vec();
            for (method m in methods) {
                let vec[arg] new_args = vec();
                for (arg a in m.inputs) {
                    new_args += vec(rec(mode=a.mode, ty=fold_ty(fld, a.ty)));
                }
                new_methods += vec(rec(proto=m.proto, ident=m.ident,
                                       inputs=new_args,
                                       output=fold_ty(fld, m.output)));
            }
            ret rewrap(ty, ty_obj(new_methods));
        }
        case (ty_var(_))        { ret fld.fold_simple_ty(ty); }
        case (ty_local(_))      { ret fld.fold_simple_ty(ty); }
        case (ty_param(_))      { ret fld.fold_simple_ty(ty); }
    }

    ret ty;
}

// Type utilities

// FIXME: remove me when == works on these tags.
fn mode_is_alias(ast.mode m) -> bool {
    alt (m) {
        case (ast.val) { ret false; }
        case (ast.alias) { ret true; }
    }
    fail;
}

fn type_is_nil(@t ty) -> bool {
    alt (ty.struct) {
        case (ty_nil) { ret true; }
        case (_) { ret false; }
    }
    fail;
}


fn type_is_structural(@t ty) -> bool {
    alt (ty.struct) {
        case (ty_tup(_))    { ret true; }
        case (ty_rec(_))    { ret true; }
        case (ty_tag(_,_))  { ret true; }
        case (ty_fn(_,_,_)) { ret true; }
        case (ty_obj(_))    { ret true; }
        case (_)            { ret false; }
    }
    fail;
}

fn type_is_sequence(@t ty) -> bool {
    alt (ty.struct) {
        case (ty_str)    { ret true; }
        case (ty_vec(_))    { ret true; }
        case (_)            { ret false; }
    }
    fail;
}

fn sequence_element_type(@t ty) -> @t {
    alt (ty.struct) {
        case (ty_str)     { ret plain_ty(ty_machine(common.ty_u8)); }
        case (ty_vec(?e)) { ret e; }
    }
    fail;
}


fn type_is_tup_like(@t ty) -> bool {
    alt (ty.struct) {
        case (ty_box(_))    { ret true; }
        case (ty_tup(_))    { ret true; }
        case (ty_rec(_))    { ret true; }
        case (ty_tag(_,_))  { ret true; }
        case (_)            { ret false; }
    }
    fail;
}

fn get_element_type(@t ty, uint i) -> @t {
    check (type_is_tup_like(ty));
    alt (ty.struct) {
        case (ty_tup(?tys)) {
            ret tys.(i);
        }
        case (ty_rec(?flds)) {
            ret flds.(i).ty;
        }
    }
    fail;
}

fn type_is_boxed(@t ty) -> bool {
    alt (ty.struct) {
        case (ty_str) { ret true; }
        case (ty_vec(_)) { ret true; }
        case (ty_box(_)) { ret true; }
        case (_) { ret false; }
    }
    fail;
}

fn type_is_scalar(@t ty) -> bool {
    alt (ty.struct) {
        case (ty_nil) { ret true; }
        case (ty_bool) { ret true; }
        case (ty_int) { ret true; }
        case (ty_uint) { ret true; }
        case (ty_machine(_)) { ret true; }
        case (ty_char) { ret true; }
        case (ty_type) { ret true; }
        case (_) { ret false; }
    }
    fail;
}

// FIXME: should we just return true for native types in
// type_is_scalar?
fn type_is_native(@t ty) -> bool {
    alt (ty.struct) {
        case (ty_native) { ret true; }
        case (_) { ret false; }
    }
    fail;
}

fn type_has_dynamic_size(@t ty) -> bool {
    alt (ty.struct) {
        case (ty_tup(?ts)) {
            auto i = 0u;
            while (i < _vec.len[@t](ts)) {
                if (type_has_dynamic_size(ts.(i))) { ret true; }
                i += 1u;
            }
        }
        case (ty_rec(?fields)) {
            auto i = 0u;
            while (i < _vec.len[field](fields)) {
                if (type_has_dynamic_size(fields.(i).ty)) { ret true; }
                i += 1u;
            }
        }
        case (ty_tag(_, ?subtys)) {
            auto i = 0u;
            while (i < _vec.len[@t](subtys)) {
                if (type_has_dynamic_size(subtys.(i))) { ret true; }
                i += 1u;
            }
        }
        case (ty_param(_)) { ret true; }
        case (_) { /* fall through */ }
    }
    ret false;
}

fn type_is_integral(@t ty) -> bool {
    alt (ty.struct) {
        case (ty_int) { ret true; }
        case (ty_uint) { ret true; }
        case (ty_machine(?m)) {
            alt (m) {
                case (common.ty_i8) { ret true; }
                case (common.ty_i16) { ret true; }
                case (common.ty_i32) { ret true; }
                case (common.ty_i64) { ret true; }

                case (common.ty_u8) { ret true; }
                case (common.ty_u16) { ret true; }
                case (common.ty_u32) { ret true; }
                case (common.ty_u64) { ret true; }
                case (_) { ret false; }
            }
        }
        case (ty_char) { ret true; }
        case (_) { ret false; }
    }
    fail;
}

fn type_is_fp(@t ty) -> bool {
    alt (ty.struct) {
        case (ty_machine(?tm)) {
            alt (tm) {
                case (common.ty_f32) { ret true; }
                case (common.ty_f64) { ret true; }
                case (_) { ret false; }
            }
        }
        case (_) { ret false; }
    }
    fail;
}

fn type_is_signed(@t ty) -> bool {
    alt (ty.struct) {
        case (ty_int) { ret true; }
        case (ty_machine(?tm)) {
            alt (tm) {
                case (common.ty_i8) { ret true; }
                case (common.ty_i16) { ret true; }
                case (common.ty_i32) { ret true; }
                case (common.ty_i64) { ret true; }
                case (_) { ret false; }
            }
        }
        case (_) { ret false; }
    }
    fail;
}

fn type_param(@t ty) -> option.t[ast.def_id] {
    alt (ty.struct) {
        case (ty_param(?id)) { ret some[ast.def_id](id); }
        case (_)             { /* fall through */        }
    }
    ret none[ast.def_id];
}

fn plain_ty(&sty st) -> @t {
    ret @rec(struct=st, mut=ast.imm, cname=none[str]);
}

fn hash_ty(&@t ty) -> uint {
    ret _str.hash(ty_to_str(ty));
}

fn eq_ty(&@t a, &@t b) -> bool {
    // FIXME: this is gross, but I think it's safe, and I don't think writing
    // a giant function to handle all the cases is necessary when structural
    // equality will someday save the day.
    ret _str.eq(ty_to_str(a), ty_to_str(b));
}

fn ann_to_type(&ast.ann ann) -> @t {
    alt (ann) {
        case (ast.ann_none) {
            // shouldn't happen, but can until the typechecker is complete
            ret plain_ty(ty_var(-1));    // FIXME: broken, broken, broken
        }
        case (ast.ann_type(?ty)) {
            ret ty;
        }
    }
}

fn count_ty_params(@t ty) -> uint {
    state obj ty_param_counter(@mutable vec[ast.def_id] param_ids) {
        fn fold_simple_ty(@t ty) -> @t {
            alt (ty.struct) {
                case (ty_param(?param_id)) {
                    for (ast.def_id other_param_id in *param_ids) {
                        if (param_id._0 == other_param_id._0 &&
                                param_id._1 == other_param_id._1) {
                            ret ty;
                        }
                    }
                    *param_ids += vec(param_id);
                }
                case (_) { /* fall through */ }
            }
            ret ty;
        }
    }

    let vec[ast.def_id] param_ids_inner = vec();
    let @mutable vec[ast.def_id] param_ids = @mutable param_ids_inner;
    fold_ty(ty_param_counter(param_ids), ty);
    ret _vec.len[ast.def_id](*param_ids);
}

// Type accessors for substructures of types

fn ty_fn_args(@t fty) -> vec[arg] {
    alt (fty.struct) {
        case (ty.ty_fn(_, ?a, _)) { ret a; }
        case (ty.ty_native_fn(_, ?a, _)) { ret a; }
    }
    fail;
}

fn ty_fn_proto(@t fty) -> ast.proto {
    alt (fty.struct) {
        case (ty.ty_fn(?p, _, _)) { ret p; }
    }
    fail;
}

fn ty_fn_ret(@t fty) -> @t {
    alt (fty.struct) {
        case (ty.ty_fn(_, _, ?r)) { ret r; }
        case (ty.ty_native_fn(_, _, ?r)) { ret r; }
    }
    fail;
}

fn is_fn_ty(@t fty) -> bool {
    alt (fty.struct) {
        case (ty.ty_fn(_, _, _)) { ret true; }
        case (ty.ty_native_fn(_, _, _)) { ret true; }
        case (_) { ret false; }
    }
    ret false;
}


// Type accessors for AST nodes

// Given an item, returns the associated type as well as a list of the IDs of
// its type parameters.
type ty_params_and_ty = tup(vec[ast.def_id], @t);
fn native_item_ty(@ast.native_item it) -> ty_params_and_ty {
    auto ty_params;
    auto result_ty;
    alt (it.node) {
        case (ast.native_item_fn(_, _, ?tps, _, ?ann)) {
            ty_params = tps;
            result_ty = ann_to_type(ann);
        }
    }
    let vec[ast.def_id] ty_param_ids = vec();
    for (ast.ty_param tp in ty_params) {
        ty_param_ids += vec(tp.id);
    }
    ret tup(ty_param_ids, result_ty);
}

fn item_ty(@ast.item it) -> ty_params_and_ty {
    let vec[ast.ty_param] ty_params;
    auto result_ty;
    alt (it.node) {
        case (ast.item_const(_, _, _, _, ?ann)) {
            ty_params = vec();
            result_ty = ann_to_type(ann);
        }
        case (ast.item_fn(_, _, ?tps, _, ?ann)) {
            ty_params = tps;
            result_ty = ann_to_type(ann);
        }
        case (ast.item_mod(_, _, _)) {
            fail;   // modules are typeless
        }
        case (ast.item_ty(_, _, ?tps, _, ?ann)) {
            ty_params = tps;
            result_ty = ann_to_type(ann);
        }
        case (ast.item_tag(_, _, ?tps, ?did)) {
            // Create a new generic polytype.
            ty_params = tps;
            let vec[@t] subtys = vec();
            for (ast.ty_param tp in tps) {
                subtys += vec(plain_ty(ty_param(tp.id)));
            }
            result_ty = plain_ty(ty_tag(did, subtys));
        }
        case (ast.item_obj(_, _, ?tps, _, ?ann)) {
            ty_params = tps;
            result_ty = ann_to_type(ann);
        }
    }

    let vec[ast.def_id] ty_param_ids = vec();
    for (ast.ty_param tp in ty_params) {
        ty_param_ids += vec(tp.id);
    }
    ret tup(ty_param_ids, result_ty);
}

fn stmt_ty(@ast.stmt s) -> @t {
    alt (s.node) {
        case (ast.stmt_expr(?e)) {
            ret expr_ty(e);
        }
        case (_) {
            ret plain_ty(ty_nil);
        }
    }
}

fn block_ty(&ast.block b) -> @t {
    alt (b.node.expr) {
        case (some[@ast.expr](?e)) { ret expr_ty(e); }
        case (none[@ast.expr])     { ret plain_ty(ty_nil); }
    }
}

fn pat_ty(@ast.pat pat) -> @t {
    alt (pat.node) {
        case (ast.pat_wild(?ann))           { ret ann_to_type(ann); }
        case (ast.pat_lit(_, ?ann))         { ret ann_to_type(ann); }
        case (ast.pat_bind(_, _, ?ann))     { ret ann_to_type(ann); }
        case (ast.pat_tag(_, _, _, ?ann))   { ret ann_to_type(ann); }
    }
    fail;   // not reached
}

fn expr_ty(@ast.expr expr) -> @t {
    alt (expr.node) {
        case (ast.expr_vec(_, ?ann))          { ret ann_to_type(ann); }
        case (ast.expr_tup(_, ?ann))          { ret ann_to_type(ann); }
        case (ast.expr_rec(_, _, ?ann))       { ret ann_to_type(ann); }
        case (ast.expr_bind(_, _, ?ann))      { ret ann_to_type(ann); }
        case (ast.expr_call(_, _, ?ann))      { ret ann_to_type(ann); }
        case (ast.expr_binary(_, _, _, ?ann)) { ret ann_to_type(ann); }
        case (ast.expr_unary(_, _, ?ann))     { ret ann_to_type(ann); }
        case (ast.expr_lit(_, ?ann))          { ret ann_to_type(ann); }
        case (ast.expr_cast(_, _, ?ann))      { ret ann_to_type(ann); }
        case (ast.expr_if(_, _, _, _, ?ann))  { ret ann_to_type(ann); }
        case (ast.expr_for(_, _, _, ?ann))    { ret ann_to_type(ann); }
        case (ast.expr_for_each(_, _, _, ?ann))
                                              { ret ann_to_type(ann); }
        case (ast.expr_while(_, _, ?ann))     { ret ann_to_type(ann); }
        case (ast.expr_do_while(_, _, ?ann))  { ret ann_to_type(ann); }
        case (ast.expr_alt(_, _, ?ann))       { ret ann_to_type(ann); }
        case (ast.expr_block(_, ?ann))        { ret ann_to_type(ann); }
        case (ast.expr_assign(_, _, ?ann))    { ret ann_to_type(ann); }
        case (ast.expr_assign_op(_, _, _, ?ann))
                                              { ret ann_to_type(ann); }
        case (ast.expr_field(_, _, ?ann))     { ret ann_to_type(ann); }
        case (ast.expr_index(_, _, ?ann))     { ret ann_to_type(ann); }
        case (ast.expr_path(_, _, ?ann))      { ret ann_to_type(ann); }
        case (ast.expr_ext(_, _, _, _, ?ann)) { ret ann_to_type(ann); }

        case (ast.expr_fail)                  { ret plain_ty(ty_nil); }
        case (ast.expr_log(_))                { ret plain_ty(ty_nil); }
        case (ast.expr_check_expr(_))         { ret plain_ty(ty_nil); }
        case (ast.expr_ret(_))                { ret plain_ty(ty_nil); }
        case (ast.expr_put(_))                { ret plain_ty(ty_nil); }
        case (ast.expr_be(_))                 { ret plain_ty(ty_nil); }
    }
    fail;
}

// Expression utilities

fn field_num(session.session sess, &span sp, &ast.ident id) -> uint {
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
                s += c;
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

fn field_idx(session.session sess, &span sp,
             &ast.ident id, vec[field] fields) -> uint {
    let uint i = 0u;
    for (field f in fields) {
        if (_str.eq(f.ident, id)) {
            ret i;
        }
        i += 1u;
    }
    sess.span_err(sp, "unknown field '" + id + "' of record");
    fail;
}

fn method_idx(session.session sess, &span sp,
              &ast.ident id, vec[method] meths) -> uint {
    let uint i = 0u;
    for (method m in meths) {
        if (_str.eq(m.ident, id)) {
            ret i;
        }
        i += 1u;
    }
    sess.span_err(sp, "unknown method '" + id + "' of obj");
    fail;
}

fn is_lval(@ast.expr expr) -> bool {
    alt (expr.node) {
        case (ast.expr_field(_,_,_))    { ret true;  }
        case (ast.expr_index(_,_,_))    { ret true;  }
        case (ast.expr_path(_,_,_))     { ret true;  }
        case (_)                        { ret false; }
    }
}

// Type unification via Robinson's algorithm (Robinson 1965). Implemented as
// described in Hoder and Voronkov:
//
//     http://www.cs.man.ac.uk/~hoderk/ubench/unification_full.pdf

fn unify(@ty.t expected, @ty.t actual, &unify_handler handler)
        -> unify_result {
    // Wraps the given type in an appropriate cname.
    //
    // TODO: This doesn't do anything yet. We should carry the cname up from
    // the expected and/or actual types when unification results in a type
    // identical to one or both of the two. The precise algorithm for this is
    // something we'll probably need to develop over time.

    // Simple structural type comparison.
    fn struct_cmp(@ty.t expected, @ty.t actual) -> unify_result {
        if (expected.struct == actual.struct) {
            ret ures_ok(expected);
        }

        ret ures_err(terr_mismatch, expected, actual);
    }

    tag fn_common_res {
        fn_common_res_err(unify_result);
        fn_common_res_ok(vec[arg], @t);
    }

    fn unify_fn_common(@hashmap[int,@ty.t] bindings,
                       @ty.t expected,
                       @ty.t actual,
                       &unify_handler handler,
                       vec[arg] expected_inputs, @t expected_output,
                       vec[arg] actual_inputs, @t actual_output)
        -> fn_common_res {
        auto expected_len = _vec.len[arg](expected_inputs);
        auto actual_len = _vec.len[arg](actual_inputs);
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

            // This should be safe, I think?
            auto result_mode;
            if (mode_is_alias(expected_input.mode) ||
                mode_is_alias(actual_input.mode)) {
                result_mode = ast.alias;
            } else {
                result_mode = ast.val;
            }

            auto result = unify_step(bindings,
                                     actual_input.ty,
                                     expected_input.ty,
                                     handler);

            alt (result) {
                case (ures_ok(?rty)) {
                    result_ins += vec(rec(mode=result_mode,
                                          ty=rty));
                }

                case (_) {
                    ret fn_common_res_err(result);
                }
            }

            i += 1u;
        }

        // Check the output.
        auto result = unify_step(bindings,
                                 expected_output,
                                 actual_output,
                                 handler);
        alt (result) {
            case (ures_ok(?rty)) {
                ret fn_common_res_ok(result_ins, rty);
            }

            case (_) {
                ret fn_common_res_err(result);
            }
        }
    }

    fn unify_fn(@hashmap[int,@ty.t] bindings,
                ast.proto e_proto,
                ast.proto a_proto,
                @ty.t expected,
                @ty.t actual,
                &unify_handler handler,
                vec[arg] expected_inputs, @t expected_output,
                vec[arg] actual_inputs, @t actual_output)
        -> unify_result {

        if (e_proto != a_proto) {
            ret ures_err(terr_mismatch, expected, actual);
        }
        auto t = unify_fn_common(bindings, expected, actual,
                                 handler, expected_inputs, expected_output,
                                 actual_inputs, actual_output);
        alt (t) {
            case (fn_common_res_err(?r)) {
                ret r;
            }
            case (fn_common_res_ok(?result_ins, ?result_out)) {
                auto t2 = plain_ty(ty.ty_fn(e_proto, result_ins, result_out));
                ret ures_ok(t2);
            }
        }
    }

    fn unify_native_fn(@hashmap[int,@ty.t] bindings,
                       ast.native_abi e_abi,
                       ast.native_abi a_abi,
                       @ty.t expected,
                       @ty.t actual,
                       &unify_handler handler,
                       vec[arg] expected_inputs, @t expected_output,
                       vec[arg] actual_inputs, @t actual_output)
        -> unify_result {
        if (e_abi != a_abi) {
            ret ures_err(terr_mismatch, expected, actual);
        }

        auto t = unify_fn_common(bindings, expected, actual,
                                 handler, expected_inputs, expected_output,
                                 actual_inputs, actual_output);
        alt (t) {
            case (fn_common_res_err(?r)) {
                ret r;
            }
            case (fn_common_res_ok(?result_ins, ?result_out)) {
                auto t2 = plain_ty(ty.ty_native_fn(e_abi, result_ins,
                                                   result_out));
                ret ures_ok(t2);
            }
        }
    }

    fn unify_obj(@hashmap[int,@ty.t] bindings,
                 @ty.t expected,
                 @ty.t actual,
                 &unify_handler handler,
                 vec[method] expected_meths,
                 vec[method] actual_meths) -> unify_result {
      let vec[method] result_meths = vec();
      let uint i = 0u;
      let uint expected_len = _vec.len[method](expected_meths);
      let uint actual_len = _vec.len[method](actual_meths);

      if (expected_len != actual_len) {
        ret ures_err(terr_meth_count, expected, actual);
      }

      // FIXME: work around buggy typestate logic for 'alt', sigh.
      fn is_ok(&unify_result r) -> bool {
          alt (r) {
              case (ures_ok(?tfn)) {
                  ret true;
              }
              case (_) {}
          }
          ret false;
      }

      fn append_if_ok(&method e_meth,
                      &unify_result r, &mutable vec[method] result_meths) {
          alt (r) {
              case (ures_ok(?tfn)) {
                  alt (tfn.struct) {
                      case (ty_fn(?proto, ?ins, ?out)) {
                          result_meths += vec(rec(inputs = ins,
                                                  output = out
                                                  with e_meth));
                      }
                  }
              }
          }
      }

      while (i < expected_len) {
        auto e_meth = expected_meths.(i);
        auto a_meth = actual_meths.(i);
        if (! _str.eq(e_meth.ident, a_meth.ident)) {
          ret ures_err(terr_obj_meths(e_meth.ident, a_meth.ident),
                       expected, actual);
        }
        auto r = unify_fn(bindings,
                          e_meth.proto, a_meth.proto,
                          expected, actual, handler,
                          e_meth.inputs, e_meth.output,
                          a_meth.inputs, a_meth.output);
        if (!is_ok(r)) {
          ret r;
        }
        append_if_ok(e_meth, r, result_meths);
        i += 1u;
      }
      auto t = plain_ty(ty_obj(result_meths));
      ret ures_ok(t);
    }

    fn resolve(@hashmap[int,@t] bindings, @t typ) -> @t {
        alt (typ.struct) {
            case (ty_var(?id)) {
                alt (bindings.find(id)) {
                    case (some[@t](?typ2)) {
                        ret resolve(bindings, typ2);
                    }
                    case (none[@t]) {
                        // fall through
                    }
                }
            }
            case (_) {
                // fall through
            }
        }
        ret typ;
    }

    fn unify_step(@hashmap[int,@ty.t] bindings, @ty.t in_expected,
                  @ty.t in_actual, &unify_handler handler) -> unify_result {

        // Resolve any bindings.
        auto expected = resolve(bindings, in_expected);
        auto actual = resolve(bindings, in_actual);

        // TODO: rewrite this using tuple pattern matching when available, to
        // avoid all this rightward drift and spikiness.

        // TODO: occurs check, to make sure we don't loop forever when
        // unifying e.g. 'a and option['a]

        alt (actual.struct) {
            // If the RHS is a variable type, then just do the appropriate
            // binding.
            case (ty.ty_var(?actual_id)) {
                bindings.insert(actual_id, expected);
                ret ures_ok(expected);
            }
            case (ty.ty_local(?actual_id)) {
                auto actual_ty = handler.resolve_local(actual_id);
                auto result = unify_step(bindings,
                                         expected,
                                         actual_ty,
                                         handler);
                alt (result) {
                    case (ures_ok(?result_ty)) {
                        handler.record_local(actual_id, result_ty);
                    }
                    case (_) { /* empty */ }
                }
                ret result;
            }
            case (ty.ty_param(?actual_id)) {
                alt (expected.struct) {

                    // These two unify via logic lower down. Fall through.
                    case (ty.ty_local(_)) { }
                    case (ty.ty_var(_)) { }

                    // More-concrete types can unify against params here.
                    case (_) {
                        ret handler.unify_actual_param(actual_id,
                                                       expected,
                                                       actual);
                    }
                }
            }
            case (_) { /* empty */ }
        }

        alt (expected.struct) {
            case (ty.ty_nil)        { ret struct_cmp(expected, actual); }
            case (ty.ty_bool)       { ret struct_cmp(expected, actual); }
            case (ty.ty_int)        { ret struct_cmp(expected, actual); }
            case (ty.ty_uint)       { ret struct_cmp(expected, actual); }
            case (ty.ty_machine(_)) { ret struct_cmp(expected, actual); }
            case (ty.ty_char)       { ret struct_cmp(expected, actual); }
            case (ty.ty_str)        { ret struct_cmp(expected, actual); }
            case (ty.ty_type)       { ret struct_cmp(expected, actual); }
            case (ty.ty_native)     { ret struct_cmp(expected, actual); }

            case (ty.ty_tag(?expected_id, ?expected_tps)) {
                alt (actual.struct) {
                    case (ty.ty_tag(?actual_id, ?actual_tps)) {
                        if (expected_id._0 != actual_id._0 ||
                                expected_id._1 != actual_id._1) {
                            ret ures_err(terr_mismatch, expected, actual);
                        }

                        // TODO: factor this cruft out, see the TODO in the
                        // ty.ty_tup case
                        let vec[@ty.t] result_tps = vec();
                        auto i = 0u;
                        auto expected_len = _vec.len[@ty.t](expected_tps);
                        while (i < expected_len) {
                            auto expected_tp = expected_tps.(i);
                            auto actual_tp = actual_tps.(i);

                            auto result = unify_step(bindings,
                                                     expected_tp,
                                                     actual_tp,
                                                     handler);

                            alt (result) {
                                case (ures_ok(?rty)) {
                                    append[@ty.t](result_tps, rty);
                                }
                                case (_) {
                                    ret result;
                                }
                            }

                            i += 1u;
                        }

                        ret ures_ok(plain_ty(ty.ty_tag(expected_id,
                                                       result_tps)));
                    }
                    case (_) { /* fall through */ }
                }

                ret ures_err(terr_mismatch, expected, actual);
            }

            case (ty.ty_box(?expected_sub)) {
                alt (actual.struct) {
                    case (ty.ty_box(?actual_sub)) {
                        auto result = unify_step(bindings,
                                                 expected_sub,
                                                 actual_sub,
                                                 handler);
                        alt (result) {
                            case (ures_ok(?result_sub)) {
                                ret ures_ok(plain_ty(ty.ty_box(result_sub)));
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

            case (ty.ty_vec(?expected_sub)) {
                alt (actual.struct) {
                    case (ty.ty_vec(?actual_sub)) {
                        auto result = unify_step(bindings,
                                                 expected_sub,
                                                 actual_sub,
                                                 handler);
                        alt (result) {
                            case (ures_ok(?result_sub)) {
                                ret ures_ok(plain_ty(ty.ty_vec(result_sub)));
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

            case (ty.ty_tup(?expected_elems)) {
                alt (actual.struct) {
                    case (ty.ty_tup(?actual_elems)) {
                        auto expected_len = _vec.len[@ty.t](expected_elems);
                        auto actual_len = _vec.len[@ty.t](actual_elems);
                        if (expected_len != actual_len) {
                            auto err = terr_tuple_size(expected_len,
                                                       actual_len);
                            ret ures_err(err, expected, actual);
                        }

                        // TODO: implement an iterator that can iterate over
                        // two arrays simultaneously.
                        let vec[@ty.t] result_elems = vec();
                        auto i = 0u;
                        while (i < expected_len) {
                            auto expected_elem = expected_elems.(i);
                            auto actual_elem = actual_elems.(i);
                            if (expected_elem.mut != actual_elem.mut) {
                                auto err = terr_tuple_mutability;
                                ret ures_err(err, expected, actual);
                            }

                            auto result = unify_step(bindings,
                                                     expected_elem,
                                                     actual_elem,
                                                     handler);
                            alt (result) {
                                case (ures_ok(?rty)) {
                                    append[@ty.t](result_elems,rty);
                                }
                                case (_) {
                                    ret result;
                                }
                            }

                            i += 1u;
                        }

                        ret ures_ok(plain_ty(ty.ty_tup(result_elems)));
                    }

                    case (_) {
                        ret ures_err(terr_mismatch, expected, actual);
                    }
                }
            }

            case (ty.ty_rec(?expected_fields)) {
                alt (actual.struct) {
                    case (ty.ty_rec(?actual_fields)) {
                        auto expected_len = _vec.len[field](expected_fields);
                        auto actual_len = _vec.len[field](actual_fields);
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
                            if (expected_field.ty.mut
                                != actual_field.ty.mut) {
                                auto err = terr_record_mutability;
                                ret ures_err(err, expected, actual);
                            }

                            if (!_str.eq(expected_field.ident,
                                        actual_field.ident)) {
                                auto err =
                                    terr_record_fields(expected_field.ident,
                                                       actual_field.ident);
                                ret ures_err(err, expected, actual);
                            }

                            auto result = unify_step(bindings,
                                                     expected_field.ty,
                                                     actual_field.ty,
                                                     handler);
                            alt (result) {
                                case (ures_ok(?rty)) {
                                    append[field]
                                        (result_fields,
                                         rec(ty=rty with expected_field));
                                }
                                case (_) {
                                    ret result;
                                }
                            }

                            i += 1u;
                        }

                        ret ures_ok(plain_ty(ty.ty_rec(result_fields)));
                    }

                    case (_) {
                        ret ures_err(terr_mismatch, expected, actual);
                    }
                }
            }

            case (ty.ty_fn(?ep, ?expected_inputs, ?expected_output)) {
                alt (actual.struct) {
                    case (ty.ty_fn(?ap, ?actual_inputs, ?actual_output)) {
                        ret unify_fn(bindings, ep, ap,
                                     expected, actual, handler,
                                     expected_inputs, expected_output,
                                     actual_inputs, actual_output);
                    }

                    case (_) {
                        ret ures_err(terr_mismatch, expected, actual);
                    }
                }
            }

            case (ty.ty_native_fn(?e_abi, ?expected_inputs,
                                  ?expected_output)) {
                alt (actual.struct) {
                    case (ty.ty_native_fn(?a_abi, ?actual_inputs,
                                          ?actual_output)) {
                        ret unify_native_fn(bindings, e_abi, a_abi,
                                            expected, actual, handler,
                                            expected_inputs, expected_output,
                                            actual_inputs, actual_output);
                    }
                    case (_) {
                        ret ures_err(terr_mismatch, expected, actual);
                    }
                }
            }

            case (ty.ty_obj(?expected_meths)) {
                alt (actual.struct) {
                    case (ty.ty_obj(?actual_meths)) {
                        ret unify_obj(bindings, expected, actual, handler,
                                      expected_meths, actual_meths);
                    }
                    case (_) {
                        ret ures_err(terr_mismatch, expected, actual);
                    }
                }
            }

            case (ty.ty_var(?expected_id)) {
                // Add a binding.
                bindings.insert(expected_id, actual);
                ret ures_ok(actual);
            }

            case (ty.ty_local(?expected_id)) {
                auto expected_ty = handler.resolve_local(expected_id);
                auto result = unify_step(bindings,
                                         expected_ty,
                                         actual,
                                         handler);
                alt (result) {
                    case (ures_ok(?result_ty)) {
                        handler.record_local(expected_id, result_ty);
                    }
                    case (_) { /* empty */ }
                }
                ret result;
            }

            case (ty.ty_param(?expected_id)) {
                ret handler.unify_expected_param(expected_id, expected,
                                                 actual);
            }
        }

        // TODO: remove me once match-exhaustiveness checking works
        fail;
    }

    // Performs type binding substitution.
    fn substitute(@hashmap[int,@t] bindings, @t typ) -> @t {
        state obj folder(@hashmap[int,@t] bindings) {
            fn fold_simple_ty(@t typ) -> @t {
                alt (typ.struct) {
                    case (ty_var(?id)) {
                        alt (bindings.find(id)) {
                            case (some[@t](?typ2)) {
                                ret substitute(bindings, typ2);
                            }
                            case (none[@t]) {
                                ret typ;
                            }
                        }
                    }
                    case (_) {
                        ret typ;
                    }
                }
            }
        }

        ret ty.fold_ty(folder(bindings), typ);
    }

    fn hash_int(&int x) -> uint { ret x as uint; }
    fn eq_int(&int a, &int b) -> bool { ret a == b; }
    auto hasher = hash_int;
    auto eqer = eq_int;
    auto bindings = @map.mk_hashmap[int,@ty.t](hasher, eqer);

    auto ures = unify_step(bindings, expected, actual, handler);
    alt (ures) {
        case (ures_ok(?t))  { ret ures_ok(substitute(bindings, t)); }
        case (_)            { ret ures;                             }
    }
    fail;   // not reached
}

fn type_err_to_str(&ty.type_err err) -> str {
    alt (err) {
        case (terr_mismatch) {
            ret "types differ";
        }
        case (terr_tuple_size(?e_sz, ?a_sz)) {
            ret "expected a tuple with " + _uint.to_str(e_sz, 10u) +
                " elements but found one with " + _uint.to_str(a_sz, 10u) +
                " elements";
        }
        case (terr_tuple_mutability) {
            ret "tuple elements differ in mutability";
        }
        case (terr_record_size(?e_sz, ?a_sz)) {
            ret "expected a record with " + _uint.to_str(e_sz, 10u) +
                " fields but found one with " + _uint.to_str(a_sz, 10u) +
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

// Type parameter resolution, used in translation and typechecking

fn resolve_ty_params(ty_params_and_ty ty_params_and_polyty,
                     @t monoty) -> vec[@t] {
    obj resolve_ty_params_handler(@hashmap[ast.def_id,@t] bindings) {
        fn resolve_local(ast.def_id id) -> @t { log "resolve local"; fail; }
        fn record_local(ast.def_id id, @t ty) { log "record local"; fail; }
        fn unify_expected_param(ast.def_id id, @t expected, @t actual)
                -> unify_result {
            bindings.insert(id, actual);
            ret ures_ok(actual);
        }
        fn unify_actual_param(ast.def_id id, @t expected, @t actual)
                -> unify_result {
            bindings.insert(id, expected);
            ret ures_ok(expected);
        }
    }

    auto bindings = @new_def_hash[@t]();
    auto handler = resolve_ty_params_handler(bindings);

    auto unify_res = unify(ty_params_and_polyty._1, monoty, handler);
    alt (unify_res) {
        case (ures_ok(_))       { /* fall through */ }
        case (ures_err(_,?exp,?act))  {
            log "resolve_ty_params mismatch: " + ty_to_str(exp) + " " +
                ty_to_str(act);
            fail;
        }
    }

    let vec[@t] result_tys = vec();
    auto ty_param_ids = ty_params_and_polyty._0;
    for (ast.def_id tp in ty_param_ids) {
        check (bindings.contains_key(tp));
        result_tys += vec(bindings.get(tp));
    }

    ret result_tys;
}

// Performs type parameter replacement using the supplied mapping from
// parameter IDs to types.
fn replace_type_params(@t typ, hashmap[ast.def_id,@t] param_map) -> @t {
    state obj param_replacer(hashmap[ast.def_id,@t] param_map) {
        fn fold_simple_ty(@t typ) -> @t {
            alt (typ.struct) {
                case (ty_param(?param_def)) {
                    if (param_map.contains_key(param_def)) {
                        ret param_map.get(param_def);
                    } else {
                        ret typ;
                    }
                }
                case (_) {
                    ret typ;
                }
            }
        }
    }
    auto replacer = param_replacer(param_map);
    ret fold_ty(replacer, typ);
}

// Substitutes the type parameters specified by @ty_params with the
// corresponding types in @bound in the given type. The two vectors must have
// the same length.
fn substitute_ty_params(vec[ast.ty_param] ty_params, vec[@t] bound, @t ty)
        -> @t {
    auto ty_param_len = _vec.len[ast.ty_param](ty_params);
    check (ty_param_len == _vec.len[@t](bound));

    auto bindings = common.new_def_hash[@t]();
    auto i = 0u;
    while (i < ty_param_len) {
        bindings.insert(ty_params.(i).id, bound.(i));
        i += 1u;
    }

    ret replace_type_params(ty, bindings);
}

// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C ../.. 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
