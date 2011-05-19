import front::ast;
import front::ast::method;
import front::ast::ann;
import front::ast::item;
import front::ast::item_fn;
import front::ast::_fn;
import front::ast::obj_field;
import front::ast::_obj;
import front::ast::stmt;
import front::ast::ident;
import front::ast::def_id;
import front::ast::ty_param;
import front::ast::crate;

import front::ast::expr;
import middle::fold::respan;
import middle::fold::new_identity_fold;
import middle::fold::fold_crate;
import middle::ty::type_is_nil;
import middle::ty::ret_ty_of_fn;
import util::common::span;
import tstate::ann::ts_ann;
import tstate::ann::empty_poststate;
import tstate::ann::true_precond;
import tstate::ann::true_postcond;
import tstate::ann::false_postcond;
import tstate::ann::precond;
import tstate::ann::postcond;
import tstate::ann::poststate;
import tstate::ann::prestate;
import tstate::ann::implies;
import tstate::ann::ann_precond;
import tstate::ann::ann_prestate;
import std::vec::map;
import std::vec;
import std::vec::slice;
import std::vec::unzip;
import std::vec::plus_option;
import std::vec::cat_options;

import std::option;
import std::option::t;
import std::option::some;
import std::option::none;

import aux::fn_ctxt;
import aux::crate_ctxt;
import aux::new_crate_ctxt;
import aux::expr_precond;
import aux::expr_prestate;
import aux::expr_poststate;
import aux::stmt_poststate;
import aux::stmt_to_ann;
import aux::num_locals;
import aux::fixed_point_states;
import aux::bitv_to_str;

import util::common::ty_to_str;
import util::common::log_stmt_err;
import aux::log_bitv_err;
import bitvectors::promises;

import annotate::annotate_crate;
import collect_locals::mk_f_to_fn_info;
import pre_post_conditions::check_item_fn;
import states::find_pre_post_state_fn;

fn check_states_expr(&fn_ctxt fcx, @expr e) -> () {
    let precond prec  = expr_precond(fcx.ccx, e);
    let prestate pres = expr_prestate(fcx.ccx, e);

    if (!implies(pres, prec)) {
        auto s = "";
        s += ("Unsatisfied precondition constraint for expression:\n");
        s += util::common::expr_to_str(e);
        s += ("Precondition: ");
        s += bitv_to_str(fcx.enclosing, prec);
        s += ("Prestate: ");
        s += bitv_to_str(fcx.enclosing, pres);
        fcx.ccx.tcx.sess.span_err(e.span, s);
    }
}

fn check_states_stmt(&fn_ctxt fcx, &stmt s) -> () {
    auto a = stmt_to_ann(fcx.ccx, s);
    let precond prec    = ann_precond(a);
    let prestate pres   = ann_prestate(a);

    /*    
      log_err("check_states_stmt:");
      log_stmt_err(s);
      log_err("prec = ");
      log_bitv_err(fcx.enclosing, prec);
      log_err("pres = ");
      log_bitv_err(fcx.enclosing, pres);
    */

    if (!implies(pres, prec)) {
        auto ss = "";
        ss += ("Unsatisfied precondition constraint for statement:\n");
        ss += util::common::stmt_to_str(s);
        ss += ("Precondition: ");
        ss += bitv_to_str(fcx.enclosing, prec);
        ss += ("Prestate: ");
        ss += bitv_to_str(fcx.enclosing, pres);
        fcx.ccx.tcx.sess.span_err(s.span, ss);
    }
}

fn check_states_against_conditions(&fn_ctxt fcx, &_fn f, &ann a) -> () {
    auto enclosing = fcx.enclosing;
    auto nv   = num_locals(enclosing);
    auto post = @empty_poststate(nv);

    fn do_one_(fn_ctxt fcx, &@stmt s, @poststate post) -> () {
        check_states_stmt(fcx, *s);
        *post = stmt_poststate(fcx.ccx, *s);
    }

    auto do_one = bind do_one_(fcx, _, post);
 
    vec::map[@stmt, ()](do_one, f.body.node.stmts);
    fn do_inner_(fn_ctxt fcx, &@expr e, @poststate post) -> () {
        check_states_expr(fcx, e);
        *post = expr_poststate(fcx.ccx, e);
    }
    auto do_inner = bind do_inner_(fcx, _, post);
    option::map[@expr, ()](do_inner, f.body.node.expr);
  
    /* Finally, check that the return value is initialized */
    if (f.proto == ast::proto_fn
        && ! promises(*post, fcx.id, enclosing)
        && ! type_is_nil(fcx.ccx.tcx,
                         ret_ty_of_fn(fcx.ccx.node_types, fcx.ccx.tcx, a)) ) {
        /* FIXME: call span_err, not span_warn, once I finish implementing
           ! annotations */
        fcx.ccx.tcx.sess.span_warn(f.body.span, "In function " + fcx.name +
          ", not all control paths return a value");
        fcx.ccx.tcx.sess.span_note(f.decl.output.span,
            "see declared return type of '" + ty_to_str(*f.decl.output) +
            "'");
    }

}

fn check_fn_states(&fn_ctxt fcx, &_fn f, &ann a) -> () {
    /* Compute the pre- and post-states for this function */
    auto g = find_pre_post_state_fn;
    fixed_point_states(fcx, g, f);
    
    /* Now compare each expr's pre-state to its precondition
       and post-state to its postcondition */
    check_states_against_conditions(fcx, f, a);
}

fn check_item_fn_state(&crate_ctxt ccx, &span sp, &ident i,
                       &_fn f, &vec[ty_param] ty_params,
                       &def_id id, &ann a) -> @item {

    /* Look up the var-to-bit-num map for this function */
    assert (ccx.fm.contains_key(id));
    auto f_info = ccx.fm.get(id);

    auto fcx = rec(enclosing=f_info, id=id, name=i, ccx=ccx);
    check_fn_states(fcx, f, a);

    /* Rebuild the same function */
    ret @respan(sp, item_fn(i, f, ty_params, id, a));
}

fn check_method_states(&crate_ctxt ccx, @method m) -> () {
    assert (ccx.fm.contains_key(m.node.id));
    auto fcx = rec(enclosing=ccx.fm.get(m.node.id),
                   id=m.node.id, name=m.node.ident, ccx=ccx);
    check_fn_states(fcx, m.node.meth, m.node.ann);
}

fn check_obj_state(&crate_ctxt ccx, &vec[obj_field] fields,
                   &vec[@method] methods,
                   &option::t[@method] dtor) -> _obj {
    fn one(crate_ctxt ccx, &@method m) -> () {
        ret check_method_states(ccx, m);
    }
    auto f = bind one(ccx,_);
    vec::map[@method, ()](f, methods);
    option::map[@method, ()](f, dtor);
    ret rec(fields=fields, methods=methods, dtor=dtor);
}

/* FIXME use walk instead of fold where possible */

fn check_crate(ty::node_type_table nt, ty::ctxt cx, @crate crate) -> @crate {
    let crate_ctxt ccx = new_crate_ctxt(nt, cx);

    /* Build the global map from function id to var-to-bit-num-map */
    mk_f_to_fn_info(ccx, crate);
  
    /* Add a blank ts_ann for every statement (and expression) */
    annotate_crate(ccx, *crate);

    /* Compute the pre and postcondition for every subexpression */
    
    auto fld = new_identity_fold[crate_ctxt]();
    fld = @rec(fold_item_fn = bind check_item_fn(_,_,_,_,_,_,_) with *fld);
    auto with_pre_postconditions =
           fold_crate[crate_ctxt](ccx, fld, crate);
    
    auto fld1 = new_identity_fold[crate_ctxt]();

    fld1 = @rec(fold_item_fn = bind check_item_fn_state(_,_,_,_,_,_,_),
                fold_obj     = bind check_obj_state(_,_,_,_)
                with *fld1);
    
    ret fold_crate[crate_ctxt](ccx, fld1, with_pre_postconditions);
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
