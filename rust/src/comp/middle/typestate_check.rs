import front::ast;
import front::ast::ann;
import front::ast::method;
import front::ast::ty;
import front::ast::mutability;
import front::ast::item;
import front::ast::block;
import front::ast::block_;
import front::ast::obj_field;
import front::ast::decl;
import front::ast::arm;
import front::ast::stmt;
import front::ast::stmt_;
import front::ast::stmt_decl;
import front::ast::stmt_expr;
import front::ast::stmt_crate_directive;
import front::ast::decl_local;
import front::ast::decl_item;
import front::ast::ident;
import front::ast::def_id;
import front::ast::ann;
import front::ast::field;
import front::ast::expr;
import front::ast::expr_call;
import front::ast::expr_vec;
import front::ast::expr_tup;
import front::ast::expr_path;
import front::ast::expr_field;
import front::ast::expr_index;
import front::ast::expr_log;
import front::ast::expr_block;
import front::ast::expr_rec;
import front::ast::expr_if;
import front::ast::expr_binary;
import front::ast::expr_unary;
import front::ast::expr_assign;
import front::ast::expr_assign_op;
import front::ast::expr_while;
import front::ast::expr_do_while;
import front::ast::expr_alt;
import front::ast::expr_lit;
import front::ast::expr_ret;
import front::ast::expr_self_method;
import front::ast::expr_bind;
import front::ast::expr_spawn;
import front::ast::expr_ext;
import front::ast::expr_fail;
import front::ast::expr_break;
import front::ast::expr_cont;
import front::ast::expr_send;
import front::ast::expr_recv;
import front::ast::expr_put;
import front::ast::expr_port;
import front::ast::expr_chan;
import front::ast::expr_be;
import front::ast::expr_check;
import front::ast::expr_assert;
import front::ast::expr_cast;
import front::ast::expr_for;
import front::ast::expr_for_each;
import front::ast::path;
import front::ast::elt;
import front::ast::crate_directive;
import front::ast::fn_decl;
import front::ast::_obj;
import front::ast::native_mod;
import front::ast::variant;
import front::ast::ty_param;
import front::ast::ty;
import front::ast::proto;
import front::ast::pat;
import front::ast::binop;
import front::ast::unop;
import front::ast::def;
import front::ast::lit;
import front::ast::init_op;
import front::ast::initializer;
import front::ast::local;
import front::ast::_fn;
import front::ast::ann_none;
import front::ast::ann_type;
import front::ast::_obj;
import front::ast::_mod;
import front::ast::crate;
import front::ast::item_fn;
import front::ast::item_obj;
import front::ast::def_local;

import middle::fold;
import middle::fold::respan;
import driver::session;
import util::common;
import util::common::span;
import util::common::spanned;
import util::common::new_str_hash;
import util::common::new_def_hash;
import util::common::uistr;
import util::common::elt_exprs;
import util::common::field_exprs;
import util::common::log_expr;
import util::common::log_expr_err;
import util::common::log_stmt;
import util::common::log_block;
import util::common::log_stmt_err;
import util::common::log_fn_err;
import util::common::log_fn;
import util::common::log_block_err;
import util::common::has_nonlocal_exits;
import util::common::decl_lhs;
import util::typestate_ann;
import util::typestate_ann::ts_ann;
import util::typestate_ann::empty_pre_post;
import util::typestate_ann::empty_poststate;
import util::typestate_ann::true_precond;
import util::typestate_ann::true_postcond;
import util::typestate_ann::false_postcond;
import util::typestate_ann::postcond;
import util::typestate_ann::precond;
import util::typestate_ann::poststate;
import util::typestate_ann::prestate;
import util::typestate_ann::pre_and_post;
import util::typestate_ann::get_pre;
import util::typestate_ann::get_post;
import util::typestate_ann::ann_precond;
import util::typestate_ann::ann_prestate;
import util::typestate_ann::set_precondition;
import util::typestate_ann::set_postcondition;
import util::typestate_ann::set_prestate;
import util::typestate_ann::set_poststate;
import util::typestate_ann::set_in_postcond;
import util::typestate_ann::set_in_poststate;
import util::typestate_ann::implies;
import util::typestate_ann::pre_and_post_state;
import util::typestate_ann::empty_states;
import util::typestate_ann::empty_prestate;
import util::typestate_ann::empty_ann;
import util::typestate_ann::extend_prestate;
import util::typestate_ann::extend_poststate;
import util::typestate_ann::relax_prestate;
import util::typestate_ann::intersect;
import util::typestate_ann::pp_clone;
import util::typestate_ann::clone;

import middle::ty;
import middle::ty::ann_to_type;
import middle::ty::arg;
import middle::ty::expr_ann;
import middle::ty::ty_to_str;

import pretty::pprust::print_block;
import pretty::pprust::print_expr;
import pretty::pprust::print_decl;
import pretty::pp::mkstate;
import std::io::stdout;
import std::io::str_writer;
import std::io::string_writer;
import std::_vec::map;
import std::_vec;
import std::_vec::len;
import std::_vec::pop;
import std::_vec::push;
import std::_vec::slice;
import std::_vec::unzip;
import std::_vec::plus_option;
import std::_vec::cat_options;
import std::option;
import std::option::t;
import std::option::some;
import std::option::none;
import std::option::from_maybe;
import std::option::maybe;
import std::option::is_none;
import std::option::get;
import std::map::hashmap;
import std::list;
import std::list::list;
import std::list::cons;
import std::list::nil;
import std::list::foldl;
import std::list::find;
import std::_uint;
import std::bitv;
import std::util::fst;
import std::util::snd;

import util::typestate_ann;
import util::typestate_ann::difference;
import util::typestate_ann::union;
import util::typestate_ann::pps_len;
import util::typestate_ann::require_and_preserve;

import resolve::def_map;

/**** debugging junk  ****/

fn bitv_to_str(fn_info enclosing, bitv::t v) -> str {
  auto s = "";

  for each (@tup(def_id, tup(uint, ident)) p in enclosing.items()) {
    if (bitv::get(v, p._1._0)) {
      s += " " + p._1._1 + " ";
    }
  }
  ret s;
}

fn log_bitv(fn_info enclosing, bitv::t v) {
    log(bitv_to_str(enclosing, v));
}

fn log_bitv_err(fn_info enclosing, bitv::t v) {
    log_err(bitv_to_str(enclosing, v));
}

fn tos (vec[uint] v) -> str {
  auto res = "";
  for (uint i in v) {
    if (i == 0u) {
      res += "0";
    }
    else {
      res += "1";
    }
  }
  ret res;
}

fn log_cond(vec[uint] v) -> () {
    log(tos(v));
}
fn log_cond_err(vec[uint] v) -> () {
    log_err(tos(v));
}

fn log_pp(&pre_and_post pp) -> () {
  auto p1 = bitv::to_vec(pp.precondition);
  auto p2 = bitv::to_vec(pp.postcondition);
  log("pre:");
  log_cond(p1);
  log("post:");
  log_cond(p2);
}

fn log_pp_err(&pre_and_post pp) -> () {
  auto p1 = bitv::to_vec(pp.precondition);
  auto p2 = bitv::to_vec(pp.postcondition);
  log_err("pre:");
  log_cond_err(p1);
  log_err("post:");
  log_cond_err(p2);
}

fn log_states(&pre_and_post_state pp) -> () {
  auto p1 = bitv::to_vec(pp.prestate);
  auto p2 = bitv::to_vec(pp.poststate);
  log("prestate:");
  log_cond(p1);
  log("poststate:");
  log_cond(p2);
}

fn log_states_err(&pre_and_post_state pp) -> () {
  auto p1 = bitv::to_vec(pp.prestate);
  auto p2 = bitv::to_vec(pp.poststate);
  log_err("prestate:");
  log_cond_err(p1);
  log_err("poststate:");
  log_cond_err(p2);
}

fn print_ident(&ident i) -> () {
  log(" " + i + " ");
}

fn print_idents(vec[ident] idents) -> () {
  if(len[ident](idents) == 0u) {
    ret;
  }
  else {
    log("an ident: " + pop[ident](idents));
    print_idents(idents);
  }
}
/**********************************************************************/
/* mapping from variable name (def_id is assumed to be for a local
   variable in a given function) to bit number 
   (also remembers the ident for error-logging purposes) */
type var_info     = tup(uint, ident);
type fn_info      = std::map::hashmap[def_id, var_info];
/* mapping from function name to fn_info map */
type fn_info_map = std::map::hashmap[def_id, fn_info];
 
fn bit_num(def_id v, fn_info m) -> uint {
  assert (m.contains_key(v));
  ret m.get(v)._0;
}
fn get_fn_info(fn_info_map fm, def_id did) -> fn_info {
    assert (fm.contains_key(did));
    ret fm.get(did);
}

fn var_is_local(def_id v, fn_info m) -> bool {
  ret (m.contains_key(v));
}

fn num_locals(fn_info m) -> uint {
  ret m.size();
}

fn collect_local(&@vec[tup(ident, def_id)] vars, &span sp, &@ast::local loc)
    -> @decl {
    log("collect_local: pushing " + loc.ident);
    _vec::push[tup(ident, def_id)](*vars, tup(loc.ident, loc.id));
    ret @respan(sp, decl_local(loc));
}

fn find_locals(_fn f) -> @vec[tup(ident,def_id)] {
  auto res = @_vec::alloc[tup(ident,def_id)](0u);

  auto fld = fold::new_identity_fold[@vec[tup(ident, def_id)]]();
  fld = @rec(fold_decl_local = bind collect_local(_,_,_) with *fld);
  auto ignore = fold::fold_fn[@vec[tup(ident, def_id)]](res, fld, f);

  ret res;
}

fn add_var(def_id v, ident nm, uint next, fn_info tbl) -> uint {
    log(nm + " |-> " + util::common::uistr(next));
  tbl.insert(v, tup(next,nm));
  ret (next + 1u);
}

/* builds a table mapping each local var defined in f
 to a bit number in the precondition/postcondition vectors */
fn mk_fn_info(_fn f) -> fn_info {
  auto res = new_def_hash[var_info]();
  let uint next = 0u;
  let vec[ast::arg] f_args = f.decl.inputs;

  /* ignore args, which we know are initialized;
     just collect locally declared vars */

  let @vec[tup(ident,def_id)] locals = find_locals(f);
  log(uistr(_vec::len[tup(ident, def_id)](*locals)) + " locals");
  for (tup(ident,def_id) p in *locals) {
    next = add_var(p._1, p._0, next, res);
  }

  ret res;
}

/* extends mk_fn_info to a function item, side-effecting the map fi from
   function IDs to fn_info maps */
fn mk_fn_info_item_fn(&fn_info_map fi, &span sp, &ident i, &ast::_fn f,
                 &vec[ast::ty_param] ty_params, &def_id id, &ann a) -> @item {
  fi.insert(id, mk_fn_info(f));
  log(i + " has " + uistr(num_locals(mk_fn_info(f))) + " local vars");
  ret @respan(sp, item_fn(i, f, ty_params, id, a));
}

/* extends mk_fn_info to an obj item, side-effecting the map fi from
   function IDs to fn_info maps */
fn mk_fn_info_item_obj(&fn_info_map fi, &span sp, &ident i, &ast::_obj o,
                       &vec[ast::ty_param] ty_params,
                       &ast::obj_def_ids odid, &ann a) -> @item {
    auto all_methods = _vec::clone[@method](o.methods);
    plus_option[@method](all_methods, o.dtor);
    for (@method m in all_methods) {
        fi.insert(m.node.id, mk_fn_info(m.node.meth));
        log(m.node.ident + " has " +
            uistr(num_locals(mk_fn_info(m.node.meth))) + " local vars");
    }
    ret @respan(sp, item_obj(i, o, ty_params, odid, a));
}

/* initializes the global fn_info_map (mapping each function ID, including
   nested locally defined functions, onto a mapping from local variable name
   to bit number) */
fn mk_f_to_fn_info(@ast::crate c) -> fn_info_map {
  auto res = new_def_hash[fn_info]();

  auto fld = fold::new_identity_fold[fn_info_map]();
  fld = @rec(fold_item_fn  = bind mk_fn_info_item_fn(_,_,_,_,_,_,_),
             fold_item_obj = bind mk_fn_info_item_obj(_,_,_,_,_,_,_)
               with *fld);
  fold::fold_crate[fn_info_map](res, fld, c);

  ret res;
}
/**** Helpers ****/
fn ann_to_ts_ann(ann a, uint nv) -> ts_ann {
  alt (a) {
    case (ann_none(_))      { ret empty_ann(nv); }
    case (ann_type(_,_,_,?t)) {
      alt (t) {
        /* Kind of inconsistent. empty_ann()s everywhere
         or an option of a ts_ann? */
        case (none[@ts_ann])     { ret empty_ann(nv); }
        case (some[@ts_ann](?t)) { ret *t; }
      }
    }
  }
}

fn ann_to_ts_ann_fail(ann a) -> option::t[@ts_ann] {
  alt (a) {
      case (ann_none(_)) { 
          log("ann_to_ts_ann_fail: didn't expect ann_none here");
          fail;
      }
      case (ann_type(_,_,_,?t)) {
          ret t;
      }
  }
}

fn ann_to_ts_ann_fail_more(ann a) -> @ts_ann {
  alt (a) {
      case (ann_none(_)) { 
          log("ann_to_ts_ann_fail: didn't expect ann_none here");
          fail;
      }
      case (ann_type(_,_,_,?t)) {
          assert (! is_none[@ts_ann](t));
          ret get[@ts_ann](t);
      }
  }
}

fn ann_to_poststate(ann a) -> poststate {
    ret (ann_to_ts_ann_fail_more(a)).states.poststate;
}

fn stmt_to_ann(&stmt s) -> option::t[@ts_ann] {
  alt (s.node) {
    case (stmt_decl(_,?a)) {
        ret ann_to_ts_ann_fail(a);
    }
    case (stmt_expr(_,?a)) {
        ret ann_to_ts_ann_fail(a);
    }
    case (stmt_crate_directive(_)) {
      ret none[@ts_ann];
    }
  }
}

/* fails if e has no annotation */
fn expr_states(@expr e) -> pre_and_post_state {
  alt (expr_ann(e)) {
    case (ann_none(_)) {
      log_err "expr_pp: the impossible happened (no annotation)";
      fail;
    }
    case (ann_type(_, _, _, ?maybe_pp)) {
      alt (maybe_pp) {
        case (none[@ts_ann]) {
          log_err "expr_pp: the impossible happened (no pre/post)";
          fail;
        }
        case (some[@ts_ann](?p)) {
          ret p.states;
        }
      }
    }
  }
}

/* fails if e has no annotation */
fn expr_pp(@expr e) -> pre_and_post {
  alt (expr_ann(e)) {
    case (ann_none(_)) {
      log_err "expr_pp: the impossible happened (no annotation)";
      fail;
    }
    case (ann_type(_, _, _, ?maybe_pp)) {
      alt (maybe_pp) {
        case (none[@ts_ann]) {
          log_err "expr_pp: the impossible happened (no pre/post)";
          fail;
        }
        case (some[@ts_ann](?p)) {
          ret p.conditions;
        }
      }
    }
  }
}

fn stmt_pp(&stmt s) -> pre_and_post {
    alt (stmt_to_ann(s)) {
        case (none[@ts_ann]) {
            log "stmt_pp: the impossible happened (no annotation)";
            fail;
        }
        case (some[@ts_ann](?p)) {
            ret p.conditions;
        }
    }
}

/* fails if b has no annotation */
/* FIXME: factor out code in the following two functions (block_ts_ann) */
fn block_pp(&block b) -> pre_and_post {
    alt (b.node.a) {
       case (ann_none(_)) {
           log_err "block_pp: the impossible happened (no ann)";
           fail;
       }
       case (ann_type(_, _,_,?t)) {
           alt (t) {
               case (none[@ts_ann]) {
                   log_err "block_pp: the impossible happened (no ty)";
                   fail;
               }
               case (some[@ts_ann](?ts)) {
                   ret ts.conditions;
               }
           }
       }
    }
}

fn block_states(&block b) -> pre_and_post_state {
    alt (b.node.a) {
       case (ann_none(_)) {
           log_err "block_pp: the impossible happened (no ann)";
           fail;
       }
       case (ann_type(_, _,_,?t)) {
           alt (t) {
               case (none[@ts_ann]) {
                   log_err "block_states: the impossible happened (no ty)";
                   fail;
               }
               case (some[@ts_ann](?ts)) {
                   ret ts.states;
               }
           }
       }
    }
}

fn stmt_states(&stmt s, uint nv) -> pre_and_post_state {
  alt (stmt_to_ann(s)) {
    case (none[@ts_ann]) {
      ret empty_states(nv);
    }
    case (some[@ts_ann](?a)) {
      ret a.states;
    }
  }
}


fn expr_precond(@expr e) -> precond {
  ret (expr_pp(e)).precondition;
}

fn expr_postcond(@expr e) -> postcond {
  ret (expr_pp(e)).postcondition;
}

fn expr_prestate(@expr e) -> prestate {
  ret (expr_states(e)).prestate;
}

fn expr_poststate(@expr e) -> poststate {
  ret (expr_states(e)).poststate;
}

/*
fn stmt_precond(&stmt s) -> precond {
  ret (stmt_pp(s)).precondition;
}

fn stmt_postcond(&stmt s) -> postcond {
  ret (stmt_pp(s)).postcondition;
}
*/

fn states_to_poststate(&pre_and_post_state ss) -> poststate {
  ret ss.poststate;
}

/*
fn stmt_prestate(&stmt s) -> prestate {
  ret (stmt_states(s)).prestate;
}
*/
fn stmt_poststate(&stmt s, uint nv) -> poststate {
  ret (stmt_states(s, nv)).poststate;
}

fn block_postcond(&block b) -> postcond {
    ret (block_pp(b)).postcondition;
}

fn block_poststate(&block b) -> poststate {
    ret (block_states(b)).poststate;
}

/* returns a new annotation where the pre_and_post is p */
fn with_pp(ann a, pre_and_post p) -> ann {
  alt (a) {
    case (ann_none(_)) {
      log("with_pp: the impossible happened");
      fail; /* shouldn't happen b/c code is typechecked */
    }
    case (ann_type(?tg, ?t, ?ps, _)) {
      ret (ann_type(tg, t, ps,
                    some[@ts_ann]
                    (@rec(conditions=p,
                          states=empty_states(pps_len(p))))));
    }
  }
}

// Given a list of pres and posts for exprs e0 ... en,
// return the precondition for evaluating each expr in order.
// So, if e0's post is {x} and e1's pre is {x, y, z}, the entire
// precondition shouldn't include x.
fn seq_preconds(fn_info enclosing, vec[pre_and_post] pps) -> precond {
  let uint sz = len[pre_and_post](pps);
  let uint num_vars = num_locals(enclosing);

  if (sz >= 1u) {
    auto first   = pps.(0);
    assert (pps_len(first) == num_vars);
    let precond rest = seq_preconds(enclosing,
                         slice[pre_and_post](pps, 1u, sz));
    difference(rest, first.postcondition);
    auto res = clone(first.precondition);
    union(res, rest);

    log("seq_preconds:");
    log("first.postcondition =");
    log_bitv(enclosing, first.postcondition);
    log("rest =");
    log_bitv(enclosing, rest);
    log("returning");
    log_bitv(enclosing, res);

    ret res;
  }
  else {
      ret true_precond(num_vars);
  }
}

/* works on either postconds or preconds
 should probably rethink the whole type synonym situation */
fn union_postconds_go(&postcond first, &vec[postcond] rest) -> postcond {
  auto sz = _vec::len[postcond](rest);

  if (sz > 0u) {
    auto other = rest.(0);
    union(first, other);
    union_postconds_go(first, slice[postcond](rest, 1u, len[postcond](rest)));
  }

  ret first;
}

fn union_postconds(uint nv, &vec[postcond] pcs) -> postcond {
  if (len[postcond](pcs) > 0u) {
      ret union_postconds_go(bitv::clone(pcs.(0)), pcs);
  }
  else {
      ret empty_prestate(nv);
  }
}

/* Gee, maybe we could use foldl or something */
fn intersect_postconds_go(&postcond first, &vec[postcond] rest) -> postcond {
  auto sz = _vec::len[postcond](rest);

  if (sz > 0u) {
    auto other = rest.(0);
    intersect(first, other);
    intersect_postconds_go(first, slice[postcond](rest, 1u,
                                                  len[postcond](rest)));
  }

  ret first;
}

fn intersect_postconds(&vec[postcond] pcs) -> postcond {
  assert (len[postcond](pcs) > 0u);

  ret intersect_postconds_go(bitv::clone(pcs.(0)), pcs);
}

/******* AST-traversing code ********/

fn find_pre_post_mod(&_mod m) -> _mod {
    log("implement find_pre_post_mod!");
    fail;
}

fn find_pre_post_state_mod(&_mod m) -> bool {
    log("implement find_pre_post_state_mod!");
    fail;
}

fn find_pre_post_native_mod(&native_mod m) -> native_mod {
    log("implement find_pre_post_native_mod");
    fail;
}

fn find_pre_post_state_native_mod(&native_mod m) -> bool {
    log("implement find_pre_post_state_native_mod!");
    fail;
}
 
fn find_pre_post_obj(&def_map dm, &fn_info_map fm, _obj o) -> () {
    fn do_a_method(def_map dm, fn_info_map fm, &@method m) -> () {
        assert (fm.contains_key(m.node.id));
        find_pre_post_fn(dm, fm, fm.get(m.node.id), m.node.meth);
    }
    auto f = bind do_a_method(dm, fm, _);
    _vec::map[@method, ()](f, o.methods);
    option::map[@method, ()](f, o.dtor);
}

fn find_pre_post_state_obj(&def_map dm, &fn_info_map fm, _obj o) -> bool {
    fn do_a_method(def_map dm, fn_info_map fm, &@method m) -> bool {
        assert (fm.contains_key(m.node.id));
        ret find_pre_post_state_fn(dm, fm, fm.get(m.node.id), m.node.meth);
    }
    auto f = bind do_a_method(dm, fm, _);
    auto flags = _vec::map[@method, bool](f, o.methods);
    auto changed = _vec::or(flags);
    changed = changed || maybe[@method, bool](false, f, o.dtor);
    ret changed;
}

fn find_pre_post_item(&def_map dm, &fn_info_map fm, &fn_info enclosing,
                      &item i) -> () {
  alt (i.node) {
    case (ast::item_const(?id, ?t, ?e, ?di, ?a)) {
        find_pre_post_expr(dm, fm, enclosing, e);
    }
    case (ast::item_fn(?id, ?f, ?ps, ?di, ?a)) {
      assert (fm.contains_key(di));
      find_pre_post_fn(dm, fm, fm.get(di), f);
    }
    case (ast::item_mod(?id, ?m, ?di)) {
      find_pre_post_mod(m);
    }
    case (ast::item_native_mod(?id, ?nm, ?di)) {
      find_pre_post_native_mod(nm);
    }
    case (ast::item_ty(_,_,_,_,_)) {
      ret;
    }
    case (ast::item_tag(_,_,_,_,_)) {
      ret;
    }
    case (ast::item_obj(?id, ?o, ?ps, ?di, ?a)) {
        find_pre_post_obj(dm, fm, o);
    }
  }
}

/* Finds the pre and postcondition for each expr in <args>;
   sets the precondition in a to be the result of combining
   the preconditions for <args>, and the postcondition in a to 
   be the union of all postconditions for <args> */
fn find_pre_post_exprs(&def_map dm, &fn_info_map fm, &fn_info enclosing,
                       &vec[@expr] args, ann a) {
    auto nv = num_locals(enclosing);

    fn do_one(def_map dm, fn_info_map fm, fn_info enclosing,
              &@expr e) -> () {
        find_pre_post_expr(dm, fm, enclosing, e);
    }
    auto f = bind do_one(dm, fm, enclosing, _);

    _vec::map[@expr, ()](f, args);

    fn get_pp(&@expr e) -> pre_and_post {
        ret expr_pp(e);
    }
    auto g = get_pp;
    auto pps = _vec::map[@expr, pre_and_post](g, args);
    auto h = get_post;

    set_pre_and_post(a,
       rec(precondition=seq_preconds(enclosing, pps),
           postcondition=union_postconds
           (nv, (_vec::map[pre_and_post, postcond](h, pps)))));
}

fn find_pre_post_loop(&def_map dm, &fn_info_map fm, &fn_info enclosing,
                      &@decl d, &@expr index, &block body, &ann a) -> () {
    find_pre_post_expr(dm, fm, enclosing, index);
    find_pre_post_block(dm, fm, enclosing, body);
    auto loop_precond = declare_var(enclosing, decl_lhs(d),
           seq_preconds(enclosing, vec(expr_pp(index),
                                       block_pp(body))));
    auto loop_postcond = intersect_postconds
        (vec(expr_postcond(index), block_postcond(body)));
    set_pre_and_post(a, rec(precondition=loop_precond,
                            postcondition=loop_postcond));
}

/* Fills in annotations as a side effect. Does not rebuild the expr */
fn find_pre_post_expr(&def_map dm, &fn_info_map fm, &fn_info enclosing,
                      @expr e) -> () {
    auto num_local_vars = num_locals(enclosing);

    log("find_pre_post_expr (num_locals =" +
        uistr(num_local_vars) + "):");
    log_expr(*e);
    
    alt (e.node) {
        case (expr_call(?operator, ?operands, ?a)) {
            auto args = _vec::clone[@expr](operands);
            _vec::push[@expr](args, operator);
            find_pre_post_exprs(dm, fm, enclosing, args, a);
        }
        case (expr_spawn(_, _, ?operator, ?operands, ?a)) {
            auto args = _vec::clone[@expr](operands);
            _vec::push[@expr](args, operator);
            find_pre_post_exprs(dm, fm, enclosing, args, a);
        }
        case (expr_vec(?args, _, ?a)) {
            find_pre_post_exprs(dm, fm, enclosing, args, a);
        }
        case (expr_tup(?elts, ?a)) {
            find_pre_post_exprs(dm, fm, enclosing, elt_exprs(elts), a);
        }
        case (expr_path(?p, ?a)) {
            auto res = empty_pre_post(num_local_vars);

            alt (dm.get(ast::ann_tag(a))) {
                case (def_local(?d_id)) {
                    auto i = bit_num(d_id, enclosing);
                    require_and_preserve(i, res);
                }
                case (_) { /* nothing to check */ }
            }

            // Otherwise, variable is global, so it must be initialized
            set_pre_and_post(a, res);
        }
        case (expr_self_method(?v, ?a)) {
            /* v is a method of the enclosing obj, so it must be
               initialized, right? */
            set_pre_and_post(a, empty_pre_post(num_local_vars));
        }
        case(expr_log(_, ?arg, ?a)) {
            find_pre_post_expr(dm, fm, enclosing, arg);
            set_pre_and_post(a, expr_pp(arg));
        }
        case (expr_chan(?arg, ?a)) {
            find_pre_post_expr(dm, fm, enclosing, arg);
            set_pre_and_post(a, expr_pp(arg));
        }
        case(expr_put(?opt, ?a)) {
            alt (opt) {
                case (some[@expr](?arg)) {
                    find_pre_post_expr(dm, fm, enclosing, arg);
                    set_pre_and_post(a, expr_pp(arg));
                }
                case (none[@expr]) {
                    set_pre_and_post(a, empty_pre_post(num_local_vars));
                }
            }
        }
        case (expr_block(?b, ?a)) {
            find_pre_post_block(dm, fm, enclosing, b);
            set_pre_and_post(a, block_pp(b));
        }
        case (expr_rec(?fields,?maybe_base,?a)) {
            auto es = field_exprs(fields);
            _vec::plus_option[@expr](es, maybe_base);
            find_pre_post_exprs(dm, fm, enclosing, es, a);
        }
        case (expr_assign(?lhs, ?rhs, ?a)) {
            alt (lhs.node) {
                case (expr_path(?p, ?a_lhs)) {
                    alt (dm.get(ast::ann_tag(a_lhs))) {
                        case (def_local(?d_id)) {
                            find_pre_post_expr(dm, fm, enclosing, rhs);
                            set_pre_and_post(a, expr_pp(rhs));
                            log("gen:");
                            log_expr(*e);
                            gen(enclosing, a, d_id);
                            ret;
                        }
                        case (_) {}
                    }
                }
                case (_) {}
            }
            // doesn't check that lhs is an lval, but
            // that's probably ok
            find_pre_post_exprs(dm, fm, enclosing, vec(lhs, rhs), a);
        }
        case (expr_recv(?lhs, ?rhs, ?a)) {
            alt (lhs.node) {
                case (expr_path(?p, ?a_lhs)) {
                    alt (dm.get(ast::ann_tag(a_lhs))) {
                        case (def_local(?d_id)) {
                            find_pre_post_expr(dm, fm, enclosing, rhs);
                            set_pre_and_post(a, expr_pp(rhs));
                            log("gen:");
                            log_expr(*e);
                            gen(enclosing, a, d_id);
                            ret;
                        }
                        case (_) {}
                    }
                }
                case (_) {}
            }
            // doesn't check that lhs is an lval, but
            // that's probably ok
            find_pre_post_exprs(dm, fm, enclosing, vec(lhs, rhs), a);
        }
        case (expr_assign_op(_, ?lhs, ?rhs, ?a)) {
            /* Different from expr_assign in that the lhs *must*
               already be initialized */
            find_pre_post_exprs(dm, fm, enclosing, vec(lhs, rhs), a);
        }
        case (expr_lit(_,?a)) {
            set_pre_and_post(a, empty_pre_post(num_local_vars));
        }
        case (expr_ret(?maybe_val, ?a)) {
            alt (maybe_val) {
                case (none[@expr]) {
                    set_pre_and_post(a,
                      rec(precondition=true_precond(num_local_vars),
                          postcondition=false_postcond(num_local_vars)));
                }
                case (some[@expr](?ret_val)) {
                    find_pre_post_expr(dm, fm, enclosing, ret_val);
                    let pre_and_post pp =
                        rec(precondition=expr_precond(ret_val),
                            postcondition=false_postcond(num_local_vars));
                    set_pre_and_post(a, pp);
                }
            }
        }
        case (expr_be(?e, ?a)) {
            find_pre_post_expr(dm, fm, enclosing, e);
            set_pre_and_post(a, rec(precondition=expr_prestate(e),
                          postcondition=false_postcond(num_local_vars)));
        }
        case (expr_if(?antec, ?conseq, ?maybe_alt, ?a)) {
            find_pre_post_expr(dm, fm, enclosing, antec);
            find_pre_post_block(dm, fm, enclosing, conseq);
            alt (maybe_alt) {
                case (none[@expr]) {
                    auto precond_res = seq_preconds(enclosing,
                                                    vec(expr_pp(antec),
                                                        block_pp(conseq)));
                    set_pre_and_post(a, rec(precondition=precond_res,
                                            postcondition=
                                            expr_poststate(antec)));
                }
                case (some[@expr](?altern)) {
                    find_pre_post_expr(dm, fm, enclosing, altern);
                    auto precond_true_case =
                        seq_preconds(enclosing,
                                     vec(expr_pp(antec), block_pp(conseq)));
                    auto postcond_true_case = union_postconds
                        (num_local_vars,
                         vec(expr_postcond(antec), block_postcond(conseq)));
                    auto precond_false_case = seq_preconds
                        (enclosing,
                         vec(expr_pp(antec), expr_pp(altern)));
                    auto postcond_false_case = union_postconds
                        (num_local_vars,
                         vec(expr_postcond(antec), expr_postcond(altern)));
                    auto precond_res = union_postconds
                        (num_local_vars,
                         vec(precond_true_case, precond_false_case));
                    auto postcond_res = intersect_postconds
                        (vec(postcond_true_case, postcond_false_case));
                    set_pre_and_post(a, rec(precondition=precond_res,
                                            postcondition=postcond_res));
                }
            }
        }
        case (expr_binary(?bop,?l,?r,?a)) {
            /* *unless* bop is lazy (e.g. and, or)? 
             FIXME */
            find_pre_post_exprs(dm, fm, enclosing, vec(l, r), a);
        }
        case (expr_send(?l, ?r, ?a)) {
            find_pre_post_exprs(dm, fm, enclosing, vec(l, r), a);
        }
        case (expr_unary(_,?operand,?a)) {
            find_pre_post_expr(dm, fm, enclosing, operand);
            set_pre_and_post(a, expr_pp(operand));
        }
        case (expr_cast(?operand, _, ?a)) {
            find_pre_post_expr(dm, fm, enclosing, operand);
            set_pre_and_post(a, expr_pp(operand));
        }
        case (expr_while(?test, ?body, ?a)) {
            find_pre_post_expr(dm, fm, enclosing, test);
            find_pre_post_block(dm, fm, enclosing, body);
            set_pre_and_post(a,
              rec(precondition=
                  seq_preconds(enclosing,
                                 vec(expr_pp(test), 
                                     block_pp(body))),
                  postcondition=
                  intersect_postconds(vec(expr_postcond(test),
                                          block_postcond(body)))));
        }
        case (expr_do_while(?body, ?test, ?a)) {
            find_pre_post_block(dm, fm, enclosing, body);
            find_pre_post_expr(dm, fm, enclosing, test);
   
            auto loop_postcond = union_postconds(num_local_vars,
                            vec(block_postcond(body), expr_postcond(test)));
            /* conservative approximination: if the body
               could break or cont, the test may never be executed */
            if (has_nonlocal_exits(body)) {
                loop_postcond = empty_poststate(num_local_vars);
            }

            set_pre_and_post(a, 
                             rec(precondition=seq_preconds(enclosing,
                                             vec(block_pp(body),
                                                 expr_pp(test))),
                   postcondition=loop_postcond));
        }
        case (expr_for(?d, ?index, ?body, ?a)) {
            find_pre_post_loop(dm, fm, enclosing, d, index, body, a);
        }
        case (expr_for_each(?d, ?index, ?body, ?a)) {
            find_pre_post_loop(dm, fm, enclosing, d, index, body, a);
        }
        case (expr_index(?e, ?sub, ?a)) {
            find_pre_post_exprs(dm, fm, enclosing, vec(e, sub), a);
        }
        case (expr_alt(?e, ?alts, ?a)) {
            find_pre_post_expr(dm, fm, enclosing, e);
            fn do_an_alt(def_map dm, fn_info_map fm, fn_info enc, &arm an_alt)
                -> pre_and_post {
                find_pre_post_block(dm, fm, enc, an_alt.block);
                ret block_pp(an_alt.block);
            }
            auto f = bind do_an_alt(dm, fm, enclosing, _);
            auto alt_pps = _vec::map[arm, pre_and_post](f, alts);
            fn combine_pp(pre_and_post antec, 
                          fn_info enclosing, &pre_and_post pp,
                          &pre_and_post next) -> pre_and_post {
                union(pp.precondition, seq_preconds(enclosing,
                                         vec(antec, next)));
                intersect(pp.postcondition, next.postcondition);
                ret pp;
            }
            auto antec_pp = pp_clone(expr_pp(e)); 
            auto e_pp  = rec(precondition=empty_prestate(num_local_vars),
                             postcondition=false_postcond(num_local_vars));
            auto g = bind combine_pp(antec_pp, enclosing, _, _);

            auto alts_overall_pp = _vec::foldl[pre_and_post, pre_and_post]
                                    (g, e_pp, alt_pps);

            set_pre_and_post(a, alts_overall_pp);
        }
        case (expr_field(?operator, _, ?a)) {
            find_pre_post_expr(dm, fm, enclosing, operator);
            set_pre_and_post(a, expr_pp(operator));
        }
        case (expr_fail(?a)) {
            set_pre_and_post(a,
                             /* if execution continues after fail,
                                then everything is true! */
               rec(precondition=empty_prestate(num_local_vars),
                   postcondition=false_postcond(num_local_vars)));
        }
        case (expr_assert(?p, ?a)) {
            find_pre_post_expr(dm, fm, enclosing, p);
            set_pre_and_post(a, expr_pp(p));
        }
        case (expr_check(?p, ?a)) {
            /* will need to change when we support arbitrary predicates... */
            find_pre_post_expr(dm, fm, enclosing, p);
            set_pre_and_post(a, expr_pp(p));
        }
        case(expr_bind(?operator, ?maybe_args, ?a)) {
            auto args = _vec::cat_options[@expr](maybe_args);
            _vec::push[@expr](args, operator); /* ??? order of eval? */
            find_pre_post_exprs(dm, fm, enclosing, args, a);
        }
        case (expr_break(?a)) {
            set_pre_and_post(a, empty_pre_post(num_local_vars));
        }
        case (expr_cont(?a)) {
            set_pre_and_post(a, empty_pre_post(num_local_vars));
        }
        case (expr_port(?a)) {
            set_pre_and_post(a, empty_pre_post(num_local_vars));
        }
        case (expr_ext(_, _, _, ?expanded, ?a)) {
            find_pre_post_expr(dm, fm, enclosing, expanded);
            set_pre_and_post(a, expr_pp(expanded));
        }
    }
}

fn gen(&fn_info enclosing, &ann a, def_id id) -> bool {
  assert (enclosing.contains_key(id));
  let uint i = (enclosing.get(id))._0;
  ret set_in_postcond(i, (ann_to_ts_ann_fail_more(a)).conditions);
}

fn declare_var(&fn_info enclosing, def_id id, prestate pre)
   -> prestate {
    assert (enclosing.contains_key(id));
    let uint i = (enclosing.get(id))._0;
    auto res = clone(pre);
    relax_prestate(i, res);
    ret res;
}

fn gen_poststate(&fn_info enclosing, &ann a, def_id id) -> bool {
  assert (enclosing.contains_key(id));
  let uint i = (enclosing.get(id))._0;

  ret set_in_poststate(i, (ann_to_ts_ann_fail_more(a)).states);
}

fn find_pre_post_stmt(&def_map dm, fn_info_map fm, &fn_info enclosing,
                      &ast::stmt s) -> () {
    log("stmt =");
    log_stmt(s);

  auto num_local_vars = num_locals(enclosing);
  alt(s.node) {
    case(ast::stmt_decl(?adecl, ?a)) {
        alt(adecl.node) {
            case(ast::decl_local(?alocal)) {
                alt(alocal.init) {
                    case(some[ast::initializer](?an_init)) {
                        find_pre_post_expr(dm, fm, enclosing, an_init.expr);
                        auto rhs_pp = expr_pp(an_init.expr);
                        set_pre_and_post(alocal.ann, rhs_pp);

                        /* Inherit ann from initializer, and add var being
                           initialized to the postcondition */
                        set_pre_and_post(a, rhs_pp);
                        /*  log("gen (decl):");
                            log_stmt(s); */
                        gen(enclosing, a, alocal.id); 
                        /*                     log_err("for stmt");
                        log_stmt(s);
                        log_err("pp = ");
                        log_pp(stmt_pp(s)); */
                    }
                    case(none[ast::initializer]) {
                        auto pp = empty_pre_post(num_local_vars);
                        set_pre_and_post(alocal.ann, pp);
                        set_pre_and_post(a, pp);
                    }
                }
            }
            case(decl_item(?anitem)) {
                auto pp = empty_pre_post(num_local_vars);
                set_pre_and_post(a, pp);
                find_pre_post_item(dm, fm, enclosing, *anitem);
            }
        }
    }
    case(stmt_expr(?e,?a)) {
        find_pre_post_expr(dm, fm, enclosing, e);
        set_pre_and_post(a, expr_pp(e));
    }    
  }
}

fn find_pre_post_block(&def_map dm, &fn_info_map fm, &fn_info enclosing,
                       block b) -> () {
    /* Want to say that if there is a break or cont in this
     block, then that invalidates the poststate upheld by
    any of the stmts after it. 
    Given that the typechecker has run, we know any break will be in
    a block that forms a loop body. So that's ok. There'll never be an
    expr_break outside a loop body, therefore, no expr_break outside a block.
    */

    /* Conservative approximation for now: This says that if a block contains
     *any* breaks or conts, then its postcondition doesn't promise anything.
     This will mean that:
     x = 0;
     break;

     won't have a postcondition that says x is initialized, but that's ok.
     */
    auto nv = num_locals(enclosing);

    fn do_one_(def_map dm, fn_info_map fm, fn_info i, &@stmt s) -> () {
        find_pre_post_stmt(dm, fm, i, *s);
        log("pre_post for stmt:");
        log_stmt(*s);
        log("is:");
        log_pp(stmt_pp(*s));
    }
    auto do_one = bind do_one_(dm, fm, enclosing, _);
    
    _vec::map[@stmt, ()](do_one, b.node.stmts);
    fn do_inner_(def_map dm, fn_info_map fm, fn_info i, &@expr e) -> () {
        find_pre_post_expr(dm, fm, i, e);
    }
    auto do_inner = bind do_inner_(dm, fm, enclosing, _);
    option::map[@expr, ()](do_inner, b.node.expr);

    let vec[pre_and_post] pps = vec();

    fn get_pp_stmt(&@stmt s) -> pre_and_post {
        ret stmt_pp(*s);
    }
    auto f = get_pp_stmt;
    pps += _vec::map[@stmt, pre_and_post](f, b.node.stmts);
    fn get_pp_expr(&@expr e) -> pre_and_post {
        ret expr_pp(e);
    }
    auto g = get_pp_expr;
    plus_option[pre_and_post](pps,
       option::map[@expr, pre_and_post](g, b.node.expr));

    auto block_precond  = seq_preconds(enclosing, pps);
    auto h = get_post;
    auto postconds =  _vec::map[pre_and_post, postcond](h, pps);
    /* A block may be empty, so this next line ensures that the postconds
       vector is non-empty. */
    _vec::push[postcond](postconds, block_precond);
    auto block_postcond = empty_poststate(nv);
    /* conservative approximation */
    if (! has_nonlocal_exits(b)) {
        block_postcond = union_postconds(nv, postconds);
    }

    set_pre_and_post(b.node.a, rec(precondition=block_precond,
                                   postcondition=block_postcond));
}

fn find_pre_post_fn(&def_map dm, &fn_info_map fm, &fn_info fi, &_fn f) -> () {
    find_pre_post_block(dm, fm, fi, f.body);
}

fn check_item_fn(&def_map dm, &fn_info_map fm, &span sp, &ident i,
                 &ast::_fn f, &vec[ast::ty_param] ty_params,
                 &def_id id, &ann a) -> @item {

    log("check_item_fn:");
    log_fn(f, i, ty_params);

  assert (fm.contains_key(id));
  find_pre_post_fn(dm, fm, fm.get(id), f);

  ret @respan(sp, ast::item_fn(i, f, ty_params, id, a));
}

fn find_pre_post_state_item(&def_map dm, &fn_info_map fm, &fn_info enclosing,
                            @item i) -> bool {
 alt (i.node) {
    case (ast::item_const(?id, ?t, ?e, ?di, ?a)) {
        ret find_pre_post_state_expr(dm, fm, enclosing,
              empty_prestate(num_locals(enclosing)), e);
    }
    case (ast::item_fn(?id, ?f, ?ps, ?di, ?a)) {
      assert (fm.contains_key(di));
      ret find_pre_post_state_fn(dm, fm, fm.get(di), f);
    }
    case (ast::item_mod(?id, ?m, ?di)) {
      ret find_pre_post_state_mod(m);
    }
    case (ast::item_native_mod(?id, ?nm, ?di)) {
      ret find_pre_post_state_native_mod(nm);
    }
    case (ast::item_ty(_,_,_,_,_)) {
      ret false;
    }
    case (ast::item_tag(_,_,_,_,_)) {
      ret false;
    }
    case (ast::item_obj(?id, ?o, ?ps, ?di, ?a)) {
        ret find_pre_post_state_obj(dm, fm, o);
    }
  }
}

fn set_prestate_ann(@ann a, prestate pre) -> bool {
  alt (*a) {
    case (ann_type(_, _,_,?ts_a)) {
      assert (! is_none[@ts_ann](ts_a));
      ret set_prestate(get[@ts_ann](ts_a), pre);
    }
    case (ann_none(_)) {
      log("set_prestate_ann: expected an ann_type here");
      fail;
    }
  }
}


fn extend_prestate_ann(ann a, prestate pre) -> bool {
  alt (a) {
    case (ann_type(_,_,_,?ts_a)) {
      assert (! is_none[@ts_ann](ts_a));
      ret extend_prestate((get[@ts_ann](ts_a)).states.prestate, pre);
    }
    case (ann_none(_)) {
      log("set_prestate_ann: expected an ann_type here");
      fail;
    }
  }
}

fn set_poststate_ann(ann a, poststate post) -> bool {
  alt (a) {
    case (ann_type(_, _,_,?ts_a)) {
      assert (! is_none[@ts_ann](ts_a));
      ret set_poststate(get[@ts_ann](ts_a), post);
    }
    case (ann_none(_)) {
      log("set_poststate_ann: expected an ann_type here");
      fail;
    }
  }
}

fn extend_poststate_ann(ann a, poststate post) -> bool {
  alt (a) {
    case (ann_type(_, _,_,?ts_a)) {
      assert (! is_none[@ts_ann](ts_a));
      ret extend_poststate((*get[@ts_ann](ts_a)).states.poststate, post);
    }
    case (ann_none(_)) {
      log("set_poststate_ann: expected an ann_type here");
      fail;
    }
  }
}

fn set_pre_and_post(&ann a, pre_and_post pp) -> () {
    alt (a) {
        case (ann_type(_, _,_,?ts_a)) {
            assert (! is_none[@ts_ann](ts_a));
            auto t = *get[@ts_ann](ts_a);
            /*  log("set_pre_and_post, old =");
            log_pp(t.conditions);
            log("new =");
            log_pp(pp);
            */
            set_precondition(t, pp.precondition);
            set_postcondition(t, pp.postcondition);
        }
        case (ann_none(_)) {
            log_err("set_pre_and_post: expected an ann_type here");
            fail;
        }
    }
}

fn seq_states(&def_map dm, &fn_info_map fm, &fn_info enclosing,
    prestate pres, vec[@expr] exprs) -> tup(bool, poststate) {
  auto changed = false;
  auto post = pres;

  for (@expr e in exprs) {
      changed = find_pre_post_state_expr(dm, fm, enclosing, post, e)
                || changed;
    post = expr_poststate(e);
  }

  ret tup(changed, post);
}

fn find_pre_post_state_exprs(&def_map dm,
                             &fn_info_map fm,
                             &fn_info enclosing,
                             &prestate pres,
                             &ann a, &vec[@expr] es) -> bool {
  auto res = seq_states(dm, fm, enclosing, pres, es);
  auto changed = res._0;
  changed = extend_prestate_ann(a, pres) || changed;
  changed = extend_poststate_ann(a, res._1) || changed;
  ret changed;
}

fn pure_exp(&ann a, &prestate p) -> bool {
  auto changed = false;
  changed = extend_prestate_ann(a, p) || changed;
  changed = extend_poststate_ann(a, p) || changed;
  ret changed;
}

fn find_pre_post_state_loop(&def_map dm, &fn_info_map fm, &fn_info enclosing,
   &prestate pres, &@decl d, &@expr index, &block body, &ann a) -> bool {
    auto changed = false;

    /* same issues as while */
    changed = extend_prestate_ann(a, pres) || changed;
    changed = find_pre_post_state_expr(dm, fm, enclosing, pres, index)
        || changed;
    /* in general, would need the intersection of
       (poststate of index, poststate of body) */
    changed = find_pre_post_state_block(dm, fm, enclosing,
                expr_poststate(index), body) || changed;
    auto res_p = intersect_postconds(vec(expr_poststate(index),
                                         block_poststate(body)));
  
    changed = extend_poststate_ann(a, res_p) || changed;
    ret changed;
}

fn find_pre_post_state_expr(&def_map dm, &fn_info_map fm, &fn_info enclosing,
                            &prestate pres, @expr e) -> bool {
  auto changed = false;
  auto num_local_vars = num_locals(enclosing);

  /* FIXME could get rid of some of the copy/paste */
  alt (e.node) {
    case (expr_vec(?elts, _, ?a)) {
      ret find_pre_post_state_exprs(dm, fm, enclosing, pres, a, elts); 
    }
    case (expr_tup(?elts, ?a)) {
      ret find_pre_post_state_exprs(dm, fm, enclosing, pres, a,
                                    elt_exprs(elts));
    }
    case (expr_call(?operator, ?operands, ?a)) {
      /* do the prestate for the rator */
      changed = find_pre_post_state_expr(dm, fm, enclosing, pres, operator)
        || changed;
      /* rands go left-to-right */
      ret(find_pre_post_state_exprs(dm, fm, enclosing,
                                    expr_poststate(operator), a, operands)
          || changed);
    }
    case (expr_spawn(_, _, ?operator, ?operands, ?a)) {
        changed = find_pre_post_state_expr(dm, fm, enclosing, pres, operator);
        ret(find_pre_post_state_exprs(dm, fm, enclosing,
                 expr_poststate(operator), a, operands)
          || changed);
    }
    case (expr_bind(?operator, ?maybe_args, ?a)) {
        changed = find_pre_post_state_expr(dm, fm, enclosing, pres, operator)
            || changed;
        ret (find_pre_post_state_exprs(dm, fm, enclosing,
          expr_poststate(operator), a, cat_options[@expr](maybe_args))
            || changed);
    }
    case (expr_path(_,?a)) {
      ret pure_exp(a, pres);
    }
    case (expr_log(_,?e,?a)) {
        /* factor out the "one exp" pattern */
        changed = find_pre_post_state_expr(dm, fm, enclosing, pres, e);
        changed = extend_prestate_ann(a, pres) || changed;
        changed = extend_poststate_ann(a, expr_poststate(e)) || changed;
        ret changed;
    }
    case (expr_chan(?e, ?a)) {
        changed = find_pre_post_state_expr(dm, fm, enclosing, pres, e);
        changed = extend_prestate_ann(a, pres) || changed;
        changed = extend_poststate_ann(a, expr_poststate(e)) || changed;
        ret changed;
    }
    case (expr_ext(_, _, _, ?expanded, ?a)) {
        changed = find_pre_post_state_expr(dm, fm, enclosing, pres, expanded);
        changed = extend_prestate_ann(a, pres) || changed;
        changed = extend_poststate_ann(a, expr_poststate(expanded))
           || changed;
        ret changed;
    }
    case (expr_put(?maybe_e, ?a)) {
        alt (maybe_e) {
            case (some[@expr](?arg)) {
                changed = find_pre_post_state_expr(dm, fm, enclosing,
                                                   pres, arg);
                changed = extend_prestate_ann(a, pres) || changed;
                changed = extend_poststate_ann(a, expr_poststate(arg))
                    || changed;
                ret changed;
            }
            case (none[@expr]) {
                ret pure_exp(a, pres);
            }
        }
    }
    case (expr_lit(?l,?a)) {
        ret pure_exp(a, pres);
    }
    case (expr_block(?b,?a)) {
        changed = find_pre_post_state_block(dm, fm, enclosing, pres, b)
           || changed;
        changed = extend_prestate_ann(a, pres) || changed;
        changed = extend_poststate_ann(a, block_poststate(b)) || changed;
        ret changed;
    }
    case (expr_rec(?fields,?maybe_base,?a)) {
        changed = find_pre_post_state_exprs(dm, fm, enclosing, pres, a,
                                            field_exprs(fields)) || changed;
        alt (maybe_base) {
            case (none[@expr]) { /* do nothing */ }
            case (some[@expr](?base)) {
                changed = find_pre_post_state_expr
                    (dm, fm, enclosing, pres, base) || changed;
                changed = extend_poststate_ann(a, expr_poststate(base))
                    || changed;
            }
        }
        ret changed;
    }
    case (expr_assign(?lhs, ?rhs, ?a)) {
        extend_prestate_ann(a, pres);

        alt (lhs.node) {
            case (expr_path(_, ?a_lhs)) {
                alt (dm.get(ast::ann_tag(a_lhs))) {
                    case (def_local(?d_id)) {
                        // assignment to local var
                        changed = pure_exp(a_lhs, pres) || changed;
                        changed = find_pre_post_state_expr
                            (dm, fm, enclosing, pres, rhs) || changed;
                        changed = extend_poststate_ann
                            (a, expr_poststate(rhs)) || changed;
                        ret gen_poststate(enclosing, a, d_id)
                            || changed;
                    }
                    case (_) {}
                }
            }
            case (_) {}
        }
        // assignment to something that must already have been init'd
        changed = find_pre_post_state_expr
            (dm, fm, enclosing, pres, lhs) || changed;
        changed = find_pre_post_state_expr
            (dm, fm, enclosing, expr_poststate(lhs), rhs) || changed;
        ret extend_poststate_ann(a, expr_poststate(rhs)) || changed;
    }
    case (expr_recv(?lhs, ?rhs, ?a)) {
        extend_prestate_ann(a, pres);

        alt (lhs.node) {
            case (expr_path(?p, ?a_lhs)) {
                alt (dm.get(ast::ann_tag(a_lhs))) {
                    case (def_local(?d_id)) {
                        // receive to local var
                        changed = pure_exp(a_lhs, pres) || changed;
                        changed = find_pre_post_state_expr
                            (dm, fm, enclosing, pres, rhs) || changed;
                        changed = extend_poststate_ann(a, expr_poststate(rhs))
                            || changed;
                        ret gen_poststate(enclosing, a, d_id) || changed;
                    }
                    case (_) {}
                }
            }
            case (_) {}
        }
        // receive to something that must already have been init'd
        changed = find_pre_post_state_expr(dm, fm, enclosing, pres, lhs)
            || changed;
        changed = find_pre_post_state_expr
            (dm, fm, enclosing, expr_poststate(lhs), rhs) || changed;
        ret extend_poststate_ann(a, expr_poststate(rhs)) || changed;
    }

    case (expr_ret(?maybe_ret_val, ?a)) {
        changed = extend_prestate_ann(a, pres) || changed;
        set_poststate_ann(a, false_postcond(num_local_vars));
        alt(maybe_ret_val) {
            case (none[@expr]) { /* do nothing */ }
            case (some[@expr](?ret_val)) {
                changed = find_pre_post_state_expr(dm, fm, enclosing,
                             pres, ret_val) || changed;
            }
        }
        ret changed;
    }
    case (expr_be(?e, ?a)) {
        changed = extend_prestate_ann(a, pres) || changed;
        set_poststate_ann(a, false_postcond(num_local_vars));
        changed = find_pre_post_state_expr
            (dm, fm, enclosing, pres, e) || changed;
        ret changed;
    }
    case (expr_if(?antec, ?conseq, ?maybe_alt, ?a)) {
        changed = extend_prestate_ann(a, pres) || changed;
        changed = find_pre_post_state_expr(dm, fm, enclosing, pres, antec)
            || changed;
        changed = find_pre_post_state_block(dm, fm, enclosing,
          expr_poststate(antec), conseq) || changed;
        alt (maybe_alt) {
            case (none[@expr]) {
                changed = extend_poststate_ann(a, expr_poststate(antec))
                    || changed;
            }
            case (some[@expr](?altern)) {
                changed = find_pre_post_state_expr(dm, fm, enclosing,
                   expr_poststate(antec), altern) || changed;
                auto poststate_res = intersect_postconds
                    (vec(block_poststate(conseq), expr_poststate(altern)));
                changed = extend_poststate_ann(a, poststate_res) || changed;
            }
        }
        log("if:");
        log_expr(*e);
        log("new prestate:");
        log_bitv(enclosing, pres);
        log("new poststate:");
        log_bitv(enclosing, expr_poststate(e));

        ret changed;
    }
    case (expr_binary(?bop, ?l, ?r, ?a)) {
        /* FIXME: what if bop is lazy? */
        changed = extend_prestate_ann(a, pres) || changed;
        changed = find_pre_post_state_expr(dm, fm, enclosing, pres, l)
                    || changed;
        changed = find_pre_post_state_expr(dm, fm,
                    enclosing, expr_poststate(l), r) || changed;
        changed = extend_poststate_ann(a, expr_poststate(r)) || changed;
        ret changed;
    }
    case (expr_send(?l, ?r, ?a)) {
        changed = extend_prestate_ann(a, pres) || changed;
        changed = find_pre_post_state_expr(dm, fm, enclosing, pres, l)
                    || changed;
        changed = find_pre_post_state_expr(dm, fm,
                    enclosing, expr_poststate(l), r) || changed;
        changed = extend_poststate_ann(a, expr_poststate(r)) || changed;
        ret changed;
    }
    case (expr_assign_op(?op, ?lhs, ?rhs, ?a)) {
        /* quite similar to binary -- should abstract this */
        changed = extend_prestate_ann(a, pres) || changed;
        changed = find_pre_post_state_expr(dm, fm, enclosing, pres, lhs)
                    || changed;
        changed = find_pre_post_state_expr(dm, fm,
                    enclosing, expr_poststate(lhs), rhs) || changed;
        changed = extend_poststate_ann(a, expr_poststate(rhs)) || changed;
        ret changed;
    }
    case (expr_while(?test, ?body, ?a)) {
        changed = extend_prestate_ann(a, pres) || changed;
        /* to handle general predicates, we need to pass in
            pres `intersect` (poststate(a)) 
         like: auto test_pres = intersect_postconds(pres, expr_postcond(a));
         However, this doesn't work right now because we would be passing
         in an all-zero prestate initially
           FIXME
           maybe need a "don't know" state in addition to 0 or 1?
        */
        changed = find_pre_post_state_expr(dm, fm, enclosing, pres, test)
            || changed;
        changed = find_pre_post_state_block(dm, fm, 
                   enclosing, expr_poststate(test), body) || changed; 
        changed = extend_poststate_ann(a,
                    intersect_postconds(vec(expr_poststate(test),
                                        block_poststate(body)))) || changed;
        ret changed;
    }
    case (expr_do_while(?body, ?test, ?a)) {
        changed = extend_prestate_ann(a, pres) || changed;
        changed = find_pre_post_state_block(dm, fm, enclosing, pres, body)
            || changed;
        changed = find_pre_post_state_expr(dm, fm, enclosing,
                     block_poststate(body), test) || changed;

        /* conservative approximination: if the body of the loop
           could break or cont, we revert to the prestate
           (TODO: could treat cont differently from break, since
           if there's a cont, the test will execute) */
        if (has_nonlocal_exits(body)) {
            changed = set_poststate_ann(a, pres) || changed;
        }
        else {
            changed = extend_poststate_ann(a, expr_poststate(test))
              || changed;
        }

        ret changed;
    }
    case (expr_for(?d, ?index, ?body, ?a)) {
        ret find_pre_post_state_loop(dm, fm, enclosing, pres, d, index,
                                     body, a);
    }
    case (expr_for_each(?d, ?index, ?body, ?a)) {
        ret find_pre_post_state_loop(dm, fm, enclosing, pres, d, index,
                                     body, a);
    }
    case (expr_index(?e, ?sub, ?a)) {
        changed = extend_prestate_ann(a, pres) || changed; 
        changed = find_pre_post_state_expr(dm, fm, enclosing, pres, e)
                  || changed;
        changed = find_pre_post_state_expr(dm, fm, enclosing,
                     expr_poststate(e), sub) || changed;
        changed = extend_poststate_ann(a, expr_poststate(sub));
        ret changed;
    }
    case (expr_alt(?e, ?alts, ?a)) {
        changed = extend_prestate_ann(a, pres) || changed; 
        changed = find_pre_post_state_expr(dm, fm, enclosing, pres, e)
                  || changed;
        auto e_post = expr_poststate(e);
        auto a_post;
        if (_vec::len[arm](alts) > 0u) {
            a_post = false_postcond(num_local_vars);
            for (arm an_alt in alts) {
                changed = find_pre_post_state_block(dm, fm, enclosing, e_post,
                                                    an_alt.block) || changed;
                changed = intersect(a_post, block_poststate(an_alt.block))
                    || changed; 
            }
        }
        else {
            // No alts; poststate is the poststate of the test
            a_post = e_post;
        }
        changed = extend_poststate_ann(a, a_post);
        ret changed;
    }
    case (expr_field(?e,_,?a)) {
        changed = find_pre_post_state_expr(dm, fm, enclosing, pres, e);
        changed = extend_prestate_ann(a, pres) || changed;
        changed = extend_poststate_ann(a, expr_poststate(e)) || changed;
        ret changed;
    }
    case (expr_unary(_,?operand,?a)) {
        changed = find_pre_post_state_expr(dm, fm, enclosing, pres, operand)
          || changed;
        changed = extend_prestate_ann(a, pres) || changed;
        changed = extend_poststate_ann(a, expr_poststate(operand))
          || changed;
        ret changed;
    }
    case (expr_cast(?operand, _, ?a)) {
          changed = find_pre_post_state_expr(dm, fm, enclosing, pres, operand)
          || changed;
        changed = extend_prestate_ann(a, pres) || changed;
        changed = extend_poststate_ann(a, expr_poststate(operand))
          || changed;
        ret changed;
    }
    case (expr_fail(?a)) {
        changed = extend_prestate_ann(a, pres) || changed;
        /* if execution continues after fail, then everything is true! woo! */
        changed = set_poststate_ann(a, false_postcond(num_local_vars))
          || changed;
        ret changed;
    }
    case (expr_assert(?p, ?a)) {
        ret pure_exp(a, pres);
    }
    case (expr_check(?p, ?a)) {
        changed = extend_prestate_ann(a, pres) || changed;
        changed = find_pre_post_state_expr(dm, fm, enclosing, pres, p)
                  || changed;
        /* FIXME: update the postcondition to reflect that p holds */
        changed = extend_poststate_ann(a, pres) || changed;
        ret changed;
    }
    case (expr_break(?a)) {
        ret pure_exp(a, pres);
    }
    case (expr_cont(?a)) {
        ret pure_exp(a, pres);
    }
    case (expr_port(?a)) {
        ret pure_exp(a, pres);
    }
    case (expr_self_method(_, ?a)) {
        ret pure_exp(a, pres);
    }
  }
}

fn find_pre_post_state_stmt(&def_map dm, &fn_info_map fm, &fn_info enclosing,
                            &prestate pres, @stmt s) -> bool {
  auto changed = false;
  auto stmt_ann_ = stmt_to_ann(*s);
  assert (!is_none[@ts_ann](stmt_ann_));
  auto stmt_ann = *(get[@ts_ann](stmt_ann_));
              log("*At beginning: stmt = ");
              log_stmt(*s);
              log("*prestate = ");
              log(bitv::to_str(stmt_ann.states.prestate));
              log("*poststate =");
              log(bitv::to_str(stmt_ann.states.poststate));
              log("*changed =");
              log(changed);
  
  alt (s.node) {
    case (stmt_decl(?adecl, ?a)) {
      alt (adecl.node) {
        case (ast::decl_local(?alocal)) {
          alt (alocal.init) {
            case (some[ast::initializer](?an_init)) {
                changed = extend_prestate(stmt_ann.states.prestate, pres)
                    || changed;
                changed = find_pre_post_state_expr
                    (dm, fm, enclosing, pres, an_init.expr) || changed;
                changed = extend_poststate(stmt_ann.states.poststate,
                                           expr_poststate(an_init.expr))
                    || changed;
                changed = gen_poststate(enclosing, a, alocal.id) || changed;
              log("Summary: stmt = ");
              log_stmt(*s);
              log("prestate = ");
              log(bitv::to_str(stmt_ann.states.prestate));
              log_bitv(enclosing, stmt_ann.states.prestate);
              log("poststate =");
              log_bitv(enclosing, stmt_ann.states.poststate);
              log("changed =");
              log(changed);
  
              ret changed;
            }
            case (none[ast::initializer]) {
              changed = extend_prestate(stmt_ann.states.prestate, pres)
                  || changed;
              changed = extend_poststate(stmt_ann.states.poststate, pres)
                  || changed;
              ret changed;
            }
          }
        }
        case (ast::decl_item(?an_item)) {
            changed = extend_prestate(stmt_ann.states.prestate, pres)
               || changed;
            changed = extend_poststate(stmt_ann.states.poststate, pres)
               || changed;
            ret find_pre_post_state_item(dm, fm, enclosing, an_item)
                || changed;
        }
      }
    }
    case (stmt_expr(?e, _)) {
      changed = find_pre_post_state_expr(dm, fm, enclosing, pres, e)
                || changed;
      changed = extend_prestate(stmt_ann.states.prestate, expr_prestate(e))
          || changed;
      changed = extend_poststate(stmt_ann.states.poststate,
                                 expr_poststate(e)) || changed;
      /*
                    log("Summary: stmt = ");
              log_stmt(*s);
              log("prestate = ");
              log(bitv::to_str(stmt_ann.states.prestate));
              log_bitv(enclosing, stmt_ann.states.prestate);
              log("poststate =");
              log(bitv::to_str(stmt_ann.states.poststate));
              log_bitv(enclosing, stmt_ann.states.poststate);
              log("changed =");
              log(changed);
      */
      ret changed;
    }
    case (_) { ret false; }
  }
}

/* Updates the pre- and post-states of statements in the block,
   returns a boolean flag saying whether any pre- or poststates changed */
fn find_pre_post_state_block(&def_map dm, &fn_info_map fm, &fn_info enclosing,
                             &prestate pres0, &block b)
  -> bool {
    
  auto changed = false;
  auto num_local_vars = num_locals(enclosing);

  /* First, set the pre-states and post-states for every expression */
  auto pres = pres0;
  
  /* Iterate over each stmt. The new prestate is <pres>. The poststate
   consist of improving <pres> with whatever variables this stmt initializes.
  Then <pres> becomes the new poststate. */ 
  for (@stmt s in b.node.stmts) {
    changed = find_pre_post_state_stmt(dm, fm, enclosing, pres, s) || changed;
    pres = stmt_poststate(*s, num_local_vars);
  }

  auto post = pres;

  alt (b.node.expr) {
    case (none[@expr]) {}
    case (some[@expr](?e)) {
      changed = find_pre_post_state_expr(dm,fm, enclosing, pres, e)
                || changed;
      post = expr_poststate(e);
    }
  }

  /*
  log_err("block:");
  log_block_err(b);
  log_err("has non-local exits?");
  log_err(has_nonlocal_exits(b));
  */

  /* conservative approximation: if a block contains a break
     or cont, we assume nothing about the poststate */
  if (has_nonlocal_exits(b)) {
      post = pres0;
  }
  
  set_prestate_ann(@b.node.a, pres0);
  set_poststate_ann(b.node.a, post);


  log("For block:");
  log_block(b);
  log("poststate = ");
  log_states(block_states(b));
  log("pres0:");
  log_bitv(enclosing, pres0);
  log("post:");
  log_bitv(enclosing, post);


  ret changed;
}

fn find_pre_post_state_fn(&def_map dm, &fn_info_map f_info, &fn_info fi,
                          &ast::_fn f) -> bool {
    /* FIXME: where do we set args as being initialized?
       What about for methods? */
    auto num_local_vars = num_locals(fi);
    ret find_pre_post_state_block(dm, f_info, fi,
                                  empty_prestate(num_local_vars), f.body);
}

fn check_states_expr(fn_info enclosing, @expr e) -> () {
  let precond prec    = expr_precond(e);
  let prestate pres   = expr_prestate(e);

  if (!implies(pres, prec)) {
      log_err("check_states_expr: Unsatisfied precondition constraint for ");
      log_expr_err(*e);
      log_err("Precondition: ");
      log_bitv_err(enclosing, prec);
      log_err("Prestate: ");
      log_bitv_err(enclosing, pres);
      fail;
  }
}

fn check_states_stmt(fn_info enclosing, &stmt s) -> () {
  alt (stmt_to_ann(s)) {
    case (none[@ts_ann]) {
      ret;
    }
    case (some[@ts_ann](?a)) {
      let precond prec    = ann_precond(*a);
      let prestate pres   = ann_prestate(*a);

      /*
      log("check_states_stmt:");
      log_stmt(s);
      log("prec = ");
      log_bitv(enclosing, prec);
      log("pres = ");
      log_bitv(enclosing, pres);
      */

      if (!implies(pres, prec)) {
        log_err("check_states_stmt: "
              + "Unsatisfied precondition constraint for ");
        log_stmt_err(s);
        log_err("Precondition: ");
        log_bitv_err(enclosing, prec);
        log_err("Prestate: ");
        log_bitv_err(enclosing, pres);
        fail;
      }
    }
  }
}

fn check_states_against_conditions(fn_info enclosing, &ast::_fn f) -> () {
  fn do_one_(fn_info i, &@stmt s) -> () {
    check_states_stmt(i, *s);
  }
  auto do_one = bind do_one_(enclosing, _);
 
  _vec::map[@stmt, ()](do_one, f.body.node.stmts);
  fn do_inner_(fn_info i, &@expr e) -> () {
    check_states_expr(i, e);
  }
  auto do_inner = bind do_inner_(enclosing, _);
  option::map[@expr, ()](do_inner, f.body.node.expr);
  
}

fn check_fn_states(&def_map dm, &fn_info_map f_info_map, &fn_info f_info,
                   &ast::_fn f) -> () {
    /* Compute the pre- and post-states for this function */
    // (Fixpoint iteration)
    while (find_pre_post_state_fn(dm, f_info_map, f_info, f)) {}
    
  /* Now compare each expr's pre-state to its precondition
     and post-state to its postcondition */
    check_states_against_conditions(f_info, f);
}

fn check_item_fn_state(def_map dm, &fn_info_map f_info_map, &span sp,
                       &ident i, &ast::_fn f, &vec[ast::ty_param] ty_params,
                       &def_id id, &ann a) -> @item {

  /* Look up the var-to-bit-num map for this function */
  assert (f_info_map.contains_key(id));
  auto f_info = f_info_map.get(id);

  check_fn_states(dm, f_info_map, f_info, f);

  /* Rebuild the same function */
  ret @respan(sp, ast::item_fn(i, f, ty_params, id, a));
}

fn check_method_states(&def_map dm, &fn_info_map f_info_map, @method m) {
    assert (f_info_map.contains_key(m.node.id));
    auto f_info = f_info_map.get(m.node.id);
    check_fn_states(dm, f_info_map, f_info, m.node.meth);
}

fn check_obj_state(def_map dm, &fn_info_map f_info_map,
                   &vec[obj_field] fields, &vec[@method] methods,
                   &option::t[@method] dtor) -> ast::_obj {
    fn one(def_map dm, fn_info_map fm, &@method m) -> () {
        ret check_method_states(dm, fm, m);
    }
    auto f = bind one(dm, f_info_map,_);
    _vec::map[@method, ()](f, methods);
    option::map[@method, ()](f, dtor);
    ret rec(fields=fields, methods=methods, dtor=dtor);
}

fn init_ann(&fn_info fi, &ann a) -> ann {
    alt (a) {
        case (ann_none(_)) {
            //            log("init_ann: shouldn't see ann_none");
            // fail;
            log("warning: init_ann: saw ann_none");
            ret a; // Would be better to fail so we can catch bugs that
            // result in an uninitialized ann -- but don't want to have to
            // write code to handle native_mods properly
        }
        case (ann_type(?tg, ?t,?ps,_)) {
            ret ann_type(tg, t, ps,
                         some[@ts_ann](@empty_ann(num_locals(fi))));
        }
    }
}

fn init_blank_ann(&() ignore, &ann a) -> ann {
    alt (a) {
        case (ann_none(_)) {
            //            log("init_blank_ann: shouldn't see ann_none");
            //fail;
            log("warning: init_blank_ann: saw ann_none");
            ret a;
        }
        case (ann_type(?tg, ?t,?ps,_)) {
            ret ann_type(tg, t, ps, some[@ts_ann](@empty_ann(0u)));
        }
    }
}

fn init_block(&fn_info fi, &span sp, &block_ b) -> block {
    log("init_block:");
    log_block(respan(sp, b));
    alt(b.a) {
        case (ann_none(_)) {
            log("init_block: shouldn't see ann_none");
            fail;
        }
        case (ann_type(_, _, ?ps, _)) {
            auto fld0 = fold::new_identity_fold[fn_info]();

            fld0 = @rec(fold_ann = bind init_ann(_,_) with *fld0);
            ret fold::fold_block[fn_info](fi, fld0, respan(sp, b)); 
        }
    }
    
}

fn item_fn_anns(&fn_info_map fm, &span sp, ident i, &ast::_fn f,
                vec[ast::ty_param] ty_params, def_id id, ann a) -> @item {

    assert (fm.contains_key(id));
    auto f_info = fm.get(id);

    log(i + " has " + uistr(num_locals(f_info)) + " local vars");

    auto fld0 = fold::new_identity_fold[fn_info]();

    fld0 = @rec(fold_ann = bind init_ann(_,_) 
                    with *fld0);

    ret fold::fold_item[fn_info]
           (f_info, fld0, @respan(sp, item_fn(i, f, ty_params, id, a))); 
}

/* This is painstakingly written as an explicit recursion b/c the
   standard ast::fold doesn't traverse in the correct order:
   consider
   fn foo() {
      fn bar() {
        auto x = 5;
        log(x);
      }
   }
   With fold, first bar() would be processed and its subexps would
   correctly be annotated with length-1 bit vectors.
   But then, the process would be repeated with (fn bar()...) as
   a subexp of foo, which has 0 local variables -- so then
   the body of bar() would be incorrectly annotated with length-0 bit
   vectors. */
fn annotate_exprs(&fn_info_map fm, &vec[@expr] es) -> vec[@expr] {
    fn one(fn_info_map fm, &@expr e) -> @expr {
        ret annotate_expr(fm, e);
    }
    auto f = bind one(fm,_);
    ret _vec::map[@expr, @expr](f, es);
}
fn annotate_elts(&fn_info_map fm, &vec[elt] es) -> vec[elt] {
    fn one(fn_info_map fm, &elt e) -> elt {
        ret rec(mut=e.mut,
                expr=annotate_expr(fm, e.expr));
    }
    auto f = bind one(fm,_);
    ret _vec::map[elt, elt](f, es);
}
fn annotate_fields(&fn_info_map fm, &vec[field] fs) -> vec[field] {
    fn one(fn_info_map fm, &field f) -> field {
        ret rec(mut=f.mut,
                 ident=f.ident,
                 expr=annotate_expr(fm, f.expr));
    }
    auto f = bind one(fm,_);
    ret _vec::map[field, field](f, fs);
}
fn annotate_option_exp(&fn_info_map fm, &option::t[@expr] o)
  -> option::t[@expr] {
    fn one(fn_info_map fm, &@expr e) -> @expr {
        ret annotate_expr(fm, e);
    }
    auto f = bind one(fm,_);
    ret option::map[@expr, @expr](f, o);
}
fn annotate_option_exprs(&fn_info_map fm, &vec[option::t[@expr]] es)
  -> vec[option::t[@expr]] {
    fn one(fn_info_map fm, &option::t[@expr] o) -> option::t[@expr] {
        ret annotate_option_exp(fm, o);
    }
    auto f = bind one(fm,_);
    ret _vec::map[option::t[@expr], option::t[@expr]](f, es);
}
fn annotate_decl(&fn_info_map fm, &@decl d) -> @decl {
    auto d1 = d.node;
    alt (d.node) {
        case (decl_local(?l)) {
            alt(l.init) {
                case (some[initializer](?init)) {
                    let option::t[initializer] an_i =
                        some[initializer]
                          (rec(expr=annotate_expr(fm, init.expr)
                                 with init));
                    let @local new_l = @rec(init=an_i with *l);
                    d1 = decl_local(new_l);
                }
                case (_) { /* do nothing */ }
            }
        }
        case (decl_item(?item)) {
            d1 = decl_item(annotate_item(fm, item));
        }
    }
    ret @respan(d.span, d1);
}
fn annotate_alts(&fn_info_map fm, &vec[arm] alts) -> vec[arm] {
    fn one(fn_info_map fm, &arm a) -> arm {
        ret rec(pat=a.pat,
                 block=annotate_block(fm, a.block));
    }
    auto f = bind one(fm,_);
    ret _vec::map[arm, arm](f, alts);

}
fn annotate_expr(&fn_info_map fm, &@expr e) -> @expr {
    auto e1 = e.node;
    alt (e.node) {
        case (expr_vec(?es, ?m, ?a)) {
            e1 = expr_vec(annotate_exprs(fm, es), m, a);
        }
        case (expr_tup(?es, ?a)) {
            e1 = expr_tup(annotate_elts(fm, es), a);
        }
        case (expr_rec(?fs, ?maybe_e, ?a)) {
            e1 = expr_rec(annotate_fields(fm, fs),
                          annotate_option_exp(fm, maybe_e), a);
        }
        case (expr_call(?e, ?es, ?a)) {
            e1 = expr_call(annotate_expr(fm, e),
                          annotate_exprs(fm, es), a);
        }
        case (expr_self_method(_,_)) {
            // no change
        }
        case (expr_bind(?e, ?maybe_es, ?a)) {
            e1 = expr_bind(annotate_expr(fm, e),
                           annotate_option_exprs(fm, maybe_es),
                           a);
        }
        case (expr_spawn(?s, ?maybe_s, ?e, ?es, ?a)) {
            e1 = expr_spawn(s, maybe_s, annotate_expr(fm, e),
                            annotate_exprs(fm, es), a);
        }
        case (expr_binary(?bop, ?w, ?x, ?a)) {
            e1 = expr_binary(bop, annotate_expr(fm, w),
                             annotate_expr(fm, x), a);
        }
        case (expr_unary(?uop, ?w, ?a)) {
            e1 = expr_unary(uop, annotate_expr(fm, w), a);
        }
        case (expr_lit(_,_)) {
            /* no change */
        }
        case (expr_cast(?e,?t,?a)) {
            e1 = expr_cast(annotate_expr(fm, e), t, a);
        }
        case (expr_if(?e, ?b, ?maybe_e, ?a)) {
            e1 = expr_if(annotate_expr(fm, e),
                         annotate_block(fm, b),
                         annotate_option_exp(fm, maybe_e), a);
        }
        case (expr_while(?e, ?b, ?a)) {
            e1 = expr_while(annotate_expr(fm, e),
                            annotate_block(fm, b), a);
        }
        case (expr_for(?d, ?e, ?b, ?a)) {
            e1 = expr_for(annotate_decl(fm, d),
                          annotate_expr(fm, e),
                          annotate_block(fm, b), a);
        }
        case (expr_for_each(?d, ?e, ?b, ?a)) {
            e1 = expr_for_each(annotate_decl(fm, d),
                          annotate_expr(fm, e),
                          annotate_block(fm, b), a);
        }
        case (expr_do_while(?b, ?e, ?a)) {
            e1 = expr_do_while(annotate_block(fm, b),
                               annotate_expr(fm, e), a);
        }
        case (expr_alt(?e, ?alts, ?a)) {
            e1 = expr_alt(annotate_expr(fm, e),
                          annotate_alts(fm, alts), a);
        }
        case (expr_block(?b, ?a)) {
            e1 = expr_block(annotate_block(fm, b), a);
        }
        case (expr_assign(?l, ?r, ?a)) {
            e1 = expr_assign(annotate_expr(fm, l), annotate_expr(fm, r), a);
        }
        case (expr_assign_op(?bop, ?l, ?r, ?a)) {
            e1 = expr_assign_op(bop,
               annotate_expr(fm, l), annotate_expr(fm, r), a);
        }
        case (expr_send(?l, ?r, ?a)) {
            e1 = expr_send(annotate_expr(fm, l),
                           annotate_expr(fm, r), a);
        }
        case (expr_recv(?l, ?r, ?a)) {
           e1 = expr_recv(annotate_expr(fm, l),
                           annotate_expr(fm, r), a);
        }
        case (expr_field(?e, ?i, ?a)) {
            e1 = expr_field(annotate_expr(fm, e),
                            i, a);
        }
        case (expr_index(?e, ?sub, ?a)) {
            e1 = expr_index(annotate_expr(fm, e),
                            annotate_expr(fm, sub), a);
        }
        case (expr_path(_,_)) {
            /* no change */
        }
        case (expr_ext(?p, ?es, ?s_opt, ?e, ?a)) {
            e1 = expr_ext(p, annotate_exprs(fm, es),
                          s_opt,
                          annotate_expr(fm, e), a);
        }
        /* no change, next 3 cases */
        case (expr_fail(_)) { }
        case (expr_break(_)) { }
        case (expr_cont(_)) { }
        case (expr_ret(?maybe_e, ?a)) {
            e1 = expr_ret(annotate_option_exp(fm, maybe_e), a);
        }
        case (expr_put(?maybe_e, ?a)) {
            e1 = expr_put(annotate_option_exp(fm, maybe_e), a);
        }
        case (expr_be(?e, ?a)) {
            e1 = expr_be(annotate_expr(fm, e), a);
        }
        case (expr_log(?n, ?e, ?a)) {
            e1 = expr_log(n, annotate_expr(fm, e), a);
        }
        case (expr_assert(?e, ?a)) {
            e1 = expr_assert(annotate_expr(fm, e), a);
        }
        case (expr_check(?e, ?a)) {
            e1 = expr_check(annotate_expr(fm, e), a);
        }
        case (expr_port(_)) { /* no change */ }
        case (expr_chan(?e, ?a)) {
            e1 = expr_chan(annotate_expr(fm, e), a);
        }
    }
    ret @respan(e.span, e1);
}

fn annotate_stmt(&fn_info_map fm, &@stmt s) -> @stmt {
    alt (s.node) {
        case (stmt_decl(?d, ?a)) {
            ret @respan(s.span, stmt_decl(annotate_decl(fm, d), a));
        }
        case (stmt_expr(?e, ?a)) {
            ret @respan(s.span, stmt_expr(annotate_expr(fm, e), a));
        }
    }
}
fn annotate_block(&fn_info_map fm, &block b) -> block {
    let vec[@stmt] new_stmts = vec();

    for (@stmt s in b.node.stmts) {
        auto new_s = annotate_stmt(fm, s);
        _vec::push[@stmt](new_stmts, new_s);
    }
    fn ann_e(fn_info_map fm, &@expr e) -> @expr {
        ret annotate_expr(fm, e);
    }
    auto f = bind ann_e(fm,_);

    auto new_e = option::map[@expr, @expr](f, b.node.expr);

    ret respan(b.span,
          rec(stmts=new_stmts, expr=new_e with b.node));
}
fn annotate_fn(&fn_info_map fm, &ast::_fn f) -> ast::_fn {
    // subexps have *already* been annotated based on
    // f's number-of-locals
    ret rec(body=annotate_block(fm, f.body) with f);
}
fn annotate_mod(&fn_info_map fm, &ast::_mod m) -> ast::_mod {
    let vec[@item] new_items = vec();

    for (@item i in m.items) {
        auto new_i = annotate_item(fm, i);
        _vec::push[@item](new_items, new_i);
    }
    ret rec(items=new_items with m);
}
fn annotate_method(&fn_info_map fm, &@method m) -> @method {
    auto f_info = get_fn_info(fm, m.node.id);
    auto fld0 = fold::new_identity_fold[fn_info]();
    fld0 = @rec(fold_ann = bind init_ann(_,_) 
                with *fld0);
    auto outer = fold::fold_method[fn_info](f_info, fld0, m);
    auto new_fn = annotate_fn(fm, outer.node.meth);
    ret @respan(m.span,
                rec(meth=new_fn with m.node));
}

fn annotate_obj(&fn_info_map fm, &ast::_obj o) -> ast::_obj {
    fn one(fn_info_map fm, &@method m) -> @method {
        ret annotate_method(fm, m);
    }
    auto f = bind one(fm,_);
    auto new_methods = _vec::map[@method, @method](f, o.methods);
    auto new_dtor    = option::map[@method, @method](f, o.dtor);
    ret rec(methods=new_methods, dtor=new_dtor with o);
}

 
// Only annotates the components of the item recursively.
fn annotate_item_inner(&fn_info_map fm, &@ast::item item) -> @ast::item {
    alt (item.node) {
        /* FIXME can't skip this case -- exprs contain blocks contain stmts,
         which contain decls */
        case (ast::item_const(_,_,_,_,_)) {
            // this has already been annotated by annotate_item
            ret item;
        }
        case (ast::item_fn(?ident, ?ff, ?tps, ?id, ?ann)) {
            ret @respan(item.span,
                        ast::item_fn(ident, annotate_fn(fm, ff),
                                     tps, id, ann));
        }
        case (ast::item_mod(?ident, ?mm, ?id)) {
            ret @respan(item.span,
                       ast::item_mod(ident, annotate_mod(fm, mm), id));
        }
        case (ast::item_native_mod(?ident, ?mm, ?id)) {
            ret item;
        }
        case (ast::item_ty(_,_,_,_,_)) {
            ret item;
        }
        case (ast::item_tag(_,_,_,_,_)) {
            ret item;
        }
        case (ast::item_obj(?ident, ?ob, ?tps, ?odid, ?ann)) {
            ret @respan(item.span,
              ast::item_obj(ident, annotate_obj(fm, ob), tps, odid, ann));
        }
    } 
}

fn annotate_item(&fn_info_map fm, &@ast::item item) -> @ast::item {
    // Using a fold, recursively set all anns in this item
    // to be blank.
    // *Then*, call annotate_item recursively to do the right
    // thing for any nested items inside this one.
    
    alt (item.node) {
        case (ast::item_const(_,_,_,_,_)) {
            auto fld0 = fold::new_identity_fold[()]();
            fld0 = @rec(fold_ann = bind init_blank_ann(_,_) 
                        with *fld0);
            ret fold::fold_item[()]((), fld0, item);
        }
        case (ast::item_fn(?i,?ff,?tps,?id,?ann)) {
            auto f_info = get_fn_info(fm, id);
            auto fld0 = fold::new_identity_fold[fn_info]();
            fld0 = @rec(fold_ann = bind init_ann(_,_) 
                        with *fld0);
            auto outer = fold::fold_item[fn_info](f_info, fld0, item);
            // now recurse into any nested items
            ret annotate_item_inner(fm, outer);
         }
        case (ast::item_mod(?i, ?mm, ?id)) {
            auto fld0 = fold::new_identity_fold[()]();
            fld0 = @rec(fold_ann = bind init_blank_ann(_,_) 
                        with *fld0);
            auto outer = fold::fold_item[()]((), fld0, item);
            ret annotate_item_inner(fm, outer);
        }
        case (ast::item_native_mod(?i, ?nm, ?id)) {
            ret item;
        }
        case (ast::item_ty(_,_,_,_,_)) {
            ret item;
        }
        case (ast::item_tag(_,_,_,_,_)) {
            ret item;
        }
        case (ast::item_obj(?i,?ob,?tps,?odid,?ann)) {
            auto fld0 = fold::new_identity_fold[()]();
            fld0 = @rec(fold_ann = bind init_blank_ann(_,_) 
                        with *fld0);
            auto outer = fold::fold_item[()]((), fld0, item);
            ret annotate_item_inner(fm, outer);
        }
    }
}

fn annotate_module(&fn_info_map fm, &ast::_mod module) -> ast::_mod {
    let vec[@item] new_items = vec();

    for (@item i in module.items) {
        auto new_item = annotate_item(fm, i);
        _vec::push[@item](new_items, new_item);
    }

    ret rec(items = new_items with module);
}

fn annotate_crate(&fn_info_map fm, &@ast::crate crate) -> @ast::crate {
    ret @respan(crate.span,
               rec(module = annotate_module(fm, crate.node.module)
                   with crate.node));
}

fn check_crate(@ast::crate crate, def_map dm) -> @ast::crate {
  /* Build the global map from function id to var-to-bit-num-map */
  auto fm = mk_f_to_fn_info(crate);

  /* Add a blank ts_ann to every statement (and expression) */
  auto with_anns = annotate_crate(fm, crate);

  /* Compute the pre and postcondition for every subexpression */
  auto fld = fold::new_identity_fold[fn_info_map]();
  fld = @rec(fold_item_fn = bind check_item_fn(dm,_,_,_,_,_,_,_) with *fld);
  auto with_pre_postconditions = fold::fold_crate[fn_info_map]
    (fm, fld, with_anns);

  auto fld1 = fold::new_identity_fold[fn_info_map]();

  fld1 = @rec(fold_item_fn = bind check_item_fn_state(dm,_,_,_,_,_,_,_),
              fold_obj     = bind check_obj_state(dm,_,_,_,_)
              with *fld1);

  ret fold::fold_crate[fn_info_map](fm, fld1,
                                    with_pre_postconditions);
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
