
import std::ivec;
import std::vec;
import std::vec::plus_option;
import std::option;
import std::option::none;
import std::option::some;

import tstate::ann::pre_and_post;
import tstate::ann::get_post;
import tstate::ann::postcond;
import tstate::ann::true_precond;
import tstate::ann::false_postcond;
import tstate::ann::empty_poststate;
import tstate::ann::require;
import tstate::ann::require_and_preserve;
import tstate::ann::union;
import tstate::ann::intersect;
import tstate::ann::pp_clone;
import tstate::ann::empty_prestate;
import tstate::ann::set_precondition;
import tstate::ann::set_postcondition;
import aux::crate_ctxt;
import aux::fn_ctxt;
import aux::num_constraints;
import aux::constraint;
import aux::expr_pp;
import aux::stmt_pp;
import aux::block_pp;
import aux::clear_pp;
import aux::clear_precond;
import aux::set_pre_and_post;
import aux::copy_pre_post;
import aux::copy_pre_post_;
import aux::expr_precond;
import aux::expr_postcond;
import aux::expr_prestate;
import aux::expr_poststate;
import aux::block_postcond;
import aux::fn_info;
import aux::log_pp;
import aux::node_id_to_def;
import aux::node_id_to_def_strict;
import aux::node_id_to_ts_ann;
import aux::set_postcond_false;
import aux::controlflow_expr;
import aux::expr_to_constr;
import aux::if_ty;
import aux::if_check;
import aux::plain_if;
import aux::forget_in_postcond;
import aux::forget_in_postcond_still_init;

import aux::constraints_expr;
import aux::substitute_constr_args;
import aux::ninit;
import aux::npred;
import aux::path_to_ident;
import aux::use_var;
import bitvectors::bit_num;
import bitvectors::promises;
import bitvectors::seq_preconds;
import bitvectors::seq_postconds;
import bitvectors::intersect_states;
import bitvectors::declare_var;
import bitvectors::gen_poststate;
import bitvectors::relax_precond_block;
import bitvectors::gen;
import syntax::ast::*;
import std::map::new_int_hash;
import util::common::new_def_hash;
import util::common::log_expr;
import util::common::log_fn;
import util::common::elt_exprs;
import util::common::field_exprs;
import util::common::has_nonlocal_exits;
import util::common::log_stmt;
import util::common::log_stmt_err;
import util::common::log_expr_err;
import util::common::log_block_err;
import util::common::log_block;
import syntax::codemap::span;
import util::ppaux::fn_ident_to_string;

fn find_pre_post_mod(&_mod m) -> _mod {
    log "implement find_pre_post_mod!";
    fail;
}

fn find_pre_post_native_mod(&native_mod m) -> native_mod {
    log "implement find_pre_post_native_mod";
    fail;
}

fn find_pre_post_obj(&crate_ctxt ccx, _obj o) {
    fn do_a_method(crate_ctxt ccx, &@method m) {
        assert (ccx.fm.contains_key(m.node.id));
        let fn_ctxt fcx =
            rec(enclosing=ccx.fm.get(m.node.id),
                id=m.node.id,
                name=m.node.ident,
                ccx=ccx);
        find_pre_post_fn(fcx, m.node.meth);
    }
    auto f = bind do_a_method(ccx, _);
    vec::map[@method, ()](f, o.methods);
    option::map[@method, ()](f, o.dtor);
}

fn find_pre_post_item(&crate_ctxt ccx, &item i) {
    alt (i.node) {
        case (item_const(_, ?e)) {
            // make a fake fcx
            let @mutable node_id[] v = @mutable ~[];
            auto fake_fcx =
                rec(enclosing=rec(constrs=@new_int_hash[constraint](),
                                  num_constraints=0u,
                                  cf=return,
                                  used_vars=v),
                    id=0,
                    name="",
                    ccx=ccx);
            find_pre_post_expr(fake_fcx, e);
        }
        case (item_fn(?f, _)) {
            assert (ccx.fm.contains_key(i.id));
            auto fcx =
                rec(enclosing=ccx.fm.get(i.id),
                    id=i.id,
                    name=i.ident,
                    ccx=ccx);
            find_pre_post_fn(fcx, f);
        }
        case (item_mod(?m)) { find_pre_post_mod(m); }
        case (item_native_mod(?nm)) { find_pre_post_native_mod(nm); }
        case (item_ty(_, _)) { ret; }
        case (item_tag(_, _)) { ret; }
        case (item_res(?dtor, ?dtor_id, _, _)) {
            auto fcx = rec(enclosing=ccx.fm.get(dtor_id),
                           id=dtor_id,
                           name=i.ident,
                           ccx=ccx);
            find_pre_post_fn(fcx, dtor);
        }
        case (item_obj(?o, _, _)) { find_pre_post_obj(ccx, o); }
    }
}


/* Finds the pre and postcondition for each expr in <args>;
   sets the precondition in a to be the result of combining
   the preconditions for <args>, and the postcondition in a to 
   be the union of all postconditions for <args> */
fn find_pre_post_exprs(&fn_ctxt fcx, &vec[@expr] args, node_id id) {
    if (vec::len[@expr](args) > 0u) {
        log "find_pre_post_exprs: oper =";
        log_expr(*args.(0));
    }
    fn do_one(fn_ctxt fcx, &@expr e) { find_pre_post_expr(fcx, e); }
    auto f = bind do_one(fcx, _);
    vec::map[@expr, ()](f, args);
    fn get_pp(crate_ctxt ccx, &@expr e) -> pre_and_post {
        ret expr_pp(ccx, e);
    }
    auto g = bind get_pp(fcx.ccx, _);
    auto pps = vec::map[@expr, pre_and_post](g, args);

    // TODO: Remove this vec->ivec conversion.
    auto pps_ivec = ~[];
    for (pre_and_post pp in pps) { pps_ivec += ~[pp]; }

    set_pre_and_post(fcx.ccx, id, seq_preconds(fcx, pps_ivec),
                     seq_postconds(fcx, ivec::map(get_post, pps_ivec)));
}

fn find_pre_post_loop(&fn_ctxt fcx, &@local l, &@expr index, &block body,
                      node_id id) {
    find_pre_post_expr(fcx, index);
    find_pre_post_block(fcx, body);
    auto v_init = rec(id=l.node.id, c=ninit(l.node.ident));
    relax_precond_block(fcx, bit_num(fcx, v_init) as node_id, body);
    
    // Hack: for-loop index variables are frequently ignored,
    // so we pretend they're used
    use_var(fcx, l.node.id);

    auto loop_precond = seq_preconds(fcx, ~[expr_pp(fcx.ccx, index),
                                            block_pp(fcx.ccx, body)]);
    auto loop_postcond = intersect_states(expr_postcond(fcx.ccx, index),
                                          block_postcond(fcx.ccx, body));
    copy_pre_post_(fcx.ccx, id, loop_precond, loop_postcond);
}

// Generates a pre/post assuming that a is the 
// annotation for an if-expression with consequent conseq
// and alternative maybe_alt
fn join_then_else(&fn_ctxt fcx, &@expr antec, &block conseq,
                  &option::t[@expr] maybe_alt, node_id id, &if_ty chck) {
    find_pre_post_expr(fcx, antec);
    find_pre_post_block(fcx, conseq);
    alt (maybe_alt) {
        case (none) {
            alt (chck) {
                case (if_check) {
                    let aux::constr c = expr_to_constr(fcx.ccx.tcx, antec);
                    gen(fcx, antec.id, c.node);
                }
                case (_) {}
            }

            auto precond_res = seq_preconds(fcx,
                ~[expr_pp(fcx.ccx, antec), block_pp(fcx.ccx, conseq)]);
            set_pre_and_post(fcx.ccx, id, precond_res,
                             expr_poststate(fcx.ccx, antec));
        }
        case (some(?altern)) {
            /*
              if check = if_check, then
              be sure that the predicate implied by antec
              is *not* true in the alternative
             */
            find_pre_post_expr(fcx, altern);
            auto precond_false_case = seq_preconds(fcx,
                ~[expr_pp(fcx.ccx, antec), expr_pp(fcx.ccx, altern)]);
            auto postcond_false_case = seq_postconds(fcx,
                ~[expr_postcond(fcx.ccx, antec),
                  expr_postcond(fcx.ccx, altern)]);

            /* Be sure to set the bit for the check condition here,
             so that it's *not* set in the alternative. */
            alt (chck) {
                case (if_check) {
                    let aux::constr c = expr_to_constr(fcx.ccx.tcx, antec);
                    gen(fcx, antec.id, c.node);
                }
                case (_) {}
            }
            auto precond_true_case = seq_preconds(fcx,
                ~[expr_pp(fcx.ccx, antec), block_pp(fcx.ccx, conseq)]);
            auto postcond_true_case = seq_postconds(fcx,
                ~[expr_postcond(fcx.ccx, antec),
                  block_postcond(fcx.ccx, conseq)]);

            auto precond_res = seq_postconds(fcx, ~[precond_true_case,
                                                    precond_false_case]);
            auto postcond_res =
                intersect_states(postcond_true_case, postcond_false_case);
            set_pre_and_post(fcx.ccx, id, precond_res, postcond_res);
        }
    }
}

fn gen_if_local(&fn_ctxt fcx, @expr lhs, @expr rhs, node_id larger_id,
                node_id new_var, &path pth) {
    alt (node_id_to_def(fcx.ccx, new_var)) {
        case (some(?d)) {
            alt (d) {
                case (def_local(?d_id)) {
                    find_pre_post_expr(fcx, rhs);
                    auto p = expr_pp(fcx.ccx, rhs);
                    set_pre_and_post(fcx.ccx, larger_id, p.precondition,
                                     p.postcondition);
                    gen(fcx, larger_id,
                        rec(id=d_id._1,
                            c=ninit(path_to_ident(fcx.ccx.tcx, pth))));
                }
                case (_) { find_pre_post_exprs(fcx, [lhs, rhs], larger_id); }
            }
        }
        case (_) { find_pre_post_exprs(fcx, [lhs, rhs], larger_id); }
    }
}


/* Fills in annotations as a side effect. Does not rebuild the expr */
fn find_pre_post_expr(&fn_ctxt fcx, @expr e) {
    auto enclosing = fcx.enclosing;
    auto num_local_vars = num_constraints(enclosing);
    fn do_rand_(fn_ctxt fcx, &@expr e) { find_pre_post_expr(fcx, e); }

    alt (e.node) {
        case (expr_call(?operator, ?operands)) {
            auto args = vec::clone(operands);
            vec::push(args, operator);

            // TODO: Remove this vec->ivec conversion.
            auto operands_ivec = ~[];
            for (@expr e in operands) { operands_ivec += ~[e]; }

            find_pre_post_exprs(fcx, args, e.id);
            /* see if the call has any constraints on its type */
            for (@ty::constr_def c in constraints_expr(fcx.ccx.tcx, operator))
                {
                    auto i =
                        bit_num(fcx,
                                rec(id=c.node.id._1,
                                    c=substitute_constr_args(fcx.ccx.tcx,
                                                             operands_ivec,
                                                             c)));
                    require(i, expr_pp(fcx.ccx, e));
                }

            /* if this is a failing call, its postcondition sets everything */
            alt (controlflow_expr(fcx.ccx, operator)) {
                case (noreturn) { set_postcond_false(fcx.ccx, e.id); }
                case (_) { }
            }
        }
        case (expr_spawn(_, _, ?operator, ?operands)) {
            auto args = vec::clone(operands);
            vec::push(args, operator);
            find_pre_post_exprs(fcx, args, e.id);
        }
        case (expr_vec(?args, _, _)) {
            find_pre_post_exprs(fcx, args, e.id);
        }
        case (expr_tup(?elts)) {
            find_pre_post_exprs(fcx, elt_exprs(elts), e.id);
        }
        case (expr_path(?p)) {
            auto rslt = expr_pp(fcx.ccx, e);
            clear_pp(rslt);
            auto df = node_id_to_def_strict(fcx.ccx.tcx, e.id);
            alt (df) {
                case (def_local(?d_id)) {
                    auto i =
                        bit_num(fcx,
                                rec(id=d_id._1,
                                    c=ninit(path_to_ident(fcx.ccx.tcx, p))));
                    use_var(fcx, d_id._1);
                    require_and_preserve(i, rslt);
                }
                case (_) {/* nothing to check */ }
            }
        }
        case (expr_self_method(?v)) { clear_pp(expr_pp(fcx.ccx, e)); }
        case (expr_log(_, ?arg)) {
            find_pre_post_expr(fcx, arg);
            copy_pre_post(fcx.ccx, e.id, arg);
        }
        case (expr_chan(?arg)) {
            find_pre_post_expr(fcx, arg);
            copy_pre_post(fcx.ccx, e.id, arg);
        }
        case (expr_put(?opt)) {
            alt (opt) {
                case (some(?arg)) {
                    find_pre_post_expr(fcx, arg);
                    copy_pre_post(fcx.ccx, e.id, arg);
                }
                case (none) { clear_pp(expr_pp(fcx.ccx, e)); }
            }
        }
        case (expr_fn(?f)) { clear_pp(expr_pp(fcx.ccx, e)); }
        case (expr_block(?b)) {
            find_pre_post_block(fcx, b);
            auto p = block_pp(fcx.ccx, b);
            set_pre_and_post(fcx.ccx, e.id, p.precondition, p.postcondition);
        }
        case (expr_rec(?fields, ?maybe_base)) {
            auto es = field_exprs(fields);
            vec::plus_option(es, maybe_base);
            find_pre_post_exprs(fcx, es, e.id);
        }
        case (expr_move(?lhs, ?rhs)) {
            alt (lhs.node) {
                case (expr_path(?p)) {
                    gen_if_local(fcx, lhs, rhs, e.id, lhs.id, p);
                }
                case (_) { find_pre_post_exprs(fcx, [lhs, rhs], e.id); }
            }
            if (is_path(rhs)) {
                forget_in_postcond(fcx, e.id, rhs.id);
            }
        }
        case (expr_swap(?lhs, ?rhs)) {
            // Both sides must already be initialized
            find_pre_post_exprs(fcx, [lhs, rhs], e.id);
            forget_in_postcond_still_init(fcx, e.id, lhs.id);
            forget_in_postcond_still_init(fcx, e.id, rhs.id);
            // Could be more precise and swap the roles of lhs and rhs
            // in any constraints
        }
        case (expr_assign(?lhs, ?rhs)) {
            alt (lhs.node) {
                case (expr_path(?p)) {
                    gen_if_local(fcx, lhs, rhs, e.id, lhs.id, p);
                    forget_in_postcond_still_init(fcx, e.id, lhs.id);
                }
                case (_) { find_pre_post_exprs(fcx, [lhs, rhs], e.id); }
            }
        }
        case (expr_recv(?lhs, ?rhs)) {
            alt (rhs.node) {
                case (expr_path(?p)) {
                    gen_if_local(fcx, rhs, lhs, e.id, rhs.id, p);
                    forget_in_postcond_still_init(fcx, e.id, lhs.id);
                 }
                case (_) {
                    // doesn't check that rhs is an lval, but
                    // that's probably ok

                    find_pre_post_exprs(fcx, [lhs, rhs], e.id);
                }
            }
        }
        case (expr_assign_op(_, ?lhs, ?rhs)) {
            /* Different from expr_assign in that the lhs *must*
               already be initialized */

            find_pre_post_exprs(fcx, [lhs, rhs], e.id);
            forget_in_postcond_still_init(fcx, e.id, lhs.id);
        }
        case (expr_lit(_)) { clear_pp(expr_pp(fcx.ccx, e)); }
        case (expr_ret(?maybe_val)) {
            alt (maybe_val) {
                case (none) {
                    clear_precond(fcx.ccx, e.id);
                    set_postcond_false(fcx.ccx, e.id);
                }
                case (some(?ret_val)) {
                    find_pre_post_expr(fcx, ret_val);
                    set_precondition(node_id_to_ts_ann(fcx.ccx, e.id),
                                     expr_precond(fcx.ccx, ret_val));
                    set_postcond_false(fcx.ccx, e.id);
                }
            }
        }
        case (expr_be(?val)) {
            find_pre_post_expr(fcx, val);
            set_pre_and_post(fcx.ccx, e.id, expr_prestate(fcx.ccx, val),
                             false_postcond(num_local_vars));
        }
        case (expr_if(?antec, ?conseq, ?maybe_alt)) {
            join_then_else(fcx, antec, conseq, maybe_alt, e.id, plain_if);
        }
        case (expr_ternary(_, _, _)) {
            find_pre_post_expr(fcx, ternary_to_if(e));
        }
        case (expr_binary(?bop, ?l, ?r)) {
            if (lazy_binop(bop)) {
                find_pre_post_expr(fcx, l);
                find_pre_post_expr(fcx, r);
                auto overall_pre = seq_preconds(fcx,
                   ~[expr_pp(fcx.ccx, l), expr_pp(fcx.ccx, r)]);
                set_precondition(node_id_to_ts_ann(fcx.ccx, e.id),
                                 overall_pre);
                set_postcondition(node_id_to_ts_ann(fcx.ccx, e.id),
                                  expr_postcond(fcx.ccx, l));
            }
            else {
                find_pre_post_exprs(fcx, [l, r], e.id);
            }
        }
        case (expr_send(?l, ?r)) {
            find_pre_post_exprs(fcx, [l, r], e.id);
        }
        case (expr_unary(_, ?operand)) {
            find_pre_post_expr(fcx, operand);
            copy_pre_post(fcx.ccx, e.id, operand);
        }
        case (expr_cast(?operand, _)) {
            find_pre_post_expr(fcx, operand);
            copy_pre_post(fcx.ccx, e.id, operand);
        }
        case (expr_while(?test, ?body)) {
            find_pre_post_expr(fcx, test);
            find_pre_post_block(fcx, body);
            set_pre_and_post(fcx.ccx, e.id,
                             seq_preconds(fcx, ~[expr_pp(fcx.ccx, test),
                                                 block_pp(fcx.ccx, body)]),
                             intersect_states(expr_postcond(fcx.ccx, test),
                                              block_postcond(fcx.ccx, body)));
        }
        case (expr_do_while(?body, ?test)) {
            find_pre_post_block(fcx, body);
            find_pre_post_expr(fcx, test);
            auto loop_postcond = seq_postconds(fcx,
                ~[block_postcond(fcx.ccx, body),
                  expr_postcond(fcx.ccx, test)]);
            /* conservative approximation: if the body
               could break or cont, the test may never be executed */

            if (has_nonlocal_exits(body)) {
                loop_postcond = empty_poststate(num_local_vars);
            }
            set_pre_and_post(fcx.ccx, e.id,
                             seq_preconds(fcx,
                                          ~[block_pp(fcx.ccx, body),
                                            expr_pp(fcx.ccx, test)]),
                             loop_postcond);
        }
        case (expr_for(?d, ?index, ?body)) {
            find_pre_post_loop(fcx, d, index, body, e.id);
        }
        case (expr_for_each(?d, ?index, ?body)) {
            find_pre_post_loop(fcx, d, index, body, e.id);
        }
        case (expr_index(?val, ?sub)) {
            find_pre_post_exprs(fcx, [val, sub], e.id);
        }
        case (expr_alt(?ex, ?alts)) {
            find_pre_post_expr(fcx, ex);
            fn do_an_alt(&fn_ctxt fcx, &arm an_alt) -> pre_and_post {
                find_pre_post_block(fcx, an_alt.block);
                ret block_pp(fcx.ccx, an_alt.block);
            }
            auto f = bind do_an_alt(fcx, _);
            auto alt_pps = vec::map[arm, pre_and_post](f, alts);
            fn combine_pp(pre_and_post antec, fn_ctxt fcx, &pre_and_post pp,
                          &pre_and_post next) -> pre_and_post {
                union(pp.precondition, seq_preconds(fcx, ~[antec, next]));
                intersect(pp.postcondition, next.postcondition);
                ret pp;
            }
            auto antec_pp = pp_clone(expr_pp(fcx.ccx, ex));
            auto e_pp =
                @rec(precondition=empty_prestate(num_local_vars),
                     postcondition=false_postcond(num_local_vars));
            auto g = bind combine_pp(antec_pp, fcx, _, _);
            auto alts_overall_pp =
                vec::foldl[pre_and_post, pre_and_post](g, e_pp, alt_pps);
            set_pre_and_post(fcx.ccx, e.id, alts_overall_pp.precondition,
                             alts_overall_pp.postcondition);
        }
        case (expr_field(?operator, _)) {
            find_pre_post_expr(fcx, operator);
            copy_pre_post(fcx.ccx, e.id, operator);
        }
        case (expr_fail(?maybe_val)) {
            auto prestate;
            alt (maybe_val) {
                case (none) { prestate = empty_prestate(num_local_vars); }
                case (some(?fail_val)) {
                    find_pre_post_expr(fcx, fail_val);
                    prestate = expr_precond(fcx.ccx, fail_val);
                }
            }
            set_pre_and_post(fcx.ccx, e.id,
                             /* if execution continues after fail,
                                then everything is true! */
                             prestate,
                             false_postcond(num_local_vars));
        }
        case (expr_assert(?p)) {
            find_pre_post_expr(fcx, p);
            copy_pre_post(fcx.ccx, e.id, p);
        }
        case (expr_check(_, ?p)) {
            find_pre_post_expr(fcx, p);
            copy_pre_post(fcx.ccx, e.id, p);
            /* predicate p holds after this expression executes */

            let aux::constr c = expr_to_constr(fcx.ccx.tcx, p);
            gen(fcx, e.id, c.node);
        }
        case (expr_if_check(?p, ?conseq, ?maybe_alt)) {
            join_then_else(fcx, p, conseq, maybe_alt, e.id, if_check);
        }

        case (expr_bind(?operator, ?maybe_args)) {
            auto args = vec::cat_options[@expr](maybe_args);
            vec::push[@expr](args, operator); /* ??? order of eval? */

            find_pre_post_exprs(fcx, args, e.id);
        }
        case (expr_break) { clear_pp(expr_pp(fcx.ccx, e)); }
        case (expr_cont) { clear_pp(expr_pp(fcx.ccx, e)); }
        case (expr_port(_)) { clear_pp(expr_pp(fcx.ccx, e)); }
        case (expr_ext(_, _, _, ?expanded)) {
            find_pre_post_expr(fcx, expanded);
            copy_pre_post(fcx.ccx, e.id, expanded);
        }
        case (expr_anon_obj(?anon_obj, _, _)) {
            alt (anon_obj.with_obj) {
                case (some(?ex)) {
                    find_pre_post_expr(fcx, ex);
                    copy_pre_post(fcx.ccx, e.id, ex);
                }
                case (none) { clear_pp(expr_pp(fcx.ccx, e)); }
            }
        }
    }
}

fn find_pre_post_stmt(&fn_ctxt fcx, &stmt s) {
    log "stmt =";
    log_stmt(s);
    alt (s.node) {
        case (stmt_decl(?adecl, ?id)) {
            alt (adecl.node) {
                case (decl_local(?alocal)) {
                    alt (alocal.node.init) {
                        case (some(?an_init)) {
                            /* LHS always becomes initialized,
                             whether or not this is a move */

                            find_pre_post_expr(fcx, an_init.expr);
                            copy_pre_post(fcx.ccx, alocal.node.id, 
                                          an_init.expr);
                            /* Inherit ann from initializer, and add var being
                               initialized to the postcondition */

                            copy_pre_post(fcx.ccx, id, an_init.expr);
                            gen(fcx, id,
                                rec(id=alocal.node.id, 
                                    c=ninit(alocal.node.ident)));
                            
                            if (an_init.op == init_move &&
                                is_path(an_init.expr)) {
                                forget_in_postcond(fcx, id, an_init.expr.id);
                            }
                        }
                        case (none) {
                            clear_pp(node_id_to_ts_ann(fcx.ccx,
                                                       alocal.node.id)
                                     .conditions);
                            clear_pp(node_id_to_ts_ann(fcx.ccx, id)
                                     .conditions);
                        }
                    }
                }
                case (decl_item(?anitem)) {
                    clear_pp(node_id_to_ts_ann(fcx.ccx, id).conditions);
                    find_pre_post_item(fcx.ccx, *anitem);
                }
            }
        }
        case (stmt_expr(?e, ?id)) {
            find_pre_post_expr(fcx, e);
            copy_pre_post(fcx.ccx, id, e);
        }
    }
}

fn find_pre_post_block(&fn_ctxt fcx, block b) {
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

    auto nv = num_constraints(fcx.enclosing);
    fn do_one_(fn_ctxt fcx, &@stmt s) {
        find_pre_post_stmt(fcx, *s);
        log "pre_post for stmt:";
        log_stmt(*s);
        log "is:";
        log_pp(stmt_pp(fcx.ccx, *s));
    }
    auto do_one = bind do_one_(fcx, _);
    vec::map[@stmt, ()](do_one, b.node.stmts);
    fn do_inner_(fn_ctxt fcx, &@expr e) { find_pre_post_expr(fcx, e); }
    auto do_inner = bind do_inner_(fcx, _);
    option::map[@expr, ()](do_inner, b.node.expr);

    let pre_and_post[] pps = ~[];
    for (@stmt s in b.node.stmts) { pps += ~[stmt_pp(fcx.ccx, *s)]; }
    alt (b.node.expr) {
      case (none) { /* no-op */ }
      case (some(?e)) { pps += ~[expr_pp(fcx.ccx, e)]; }
    }

    auto block_precond = seq_preconds(fcx, pps);

    auto postconds = ~[];
    for (pre_and_post pp in pps) { postconds += ~[get_post(pp)]; }

    /* A block may be empty, so this next line ensures that the postconds
       vector is non-empty. */
    postconds += ~[block_precond];

    auto block_postcond = empty_poststate(nv);
    /* conservative approximation */

    if (!has_nonlocal_exits(b)) {
        block_postcond = seq_postconds(fcx, postconds);
    }
    set_pre_and_post(fcx.ccx, b.node.id, block_precond, block_postcond);
}

fn find_pre_post_fn(&fn_ctxt fcx, &_fn f) {
    // hack
    use_var(fcx, fcx.id);

    find_pre_post_block(fcx, f.body);

    // Treat the tail expression as a return statement
    alt (f.body.node.expr) {
        case (some(?tailexpr)) {
            set_postcond_false(fcx.ccx, tailexpr.id);
        }
        case (none) {/* fallthrough */ }
    }
}

fn fn_pre_post(crate_ctxt ccx, &_fn f, &vec[ty_param] tps,
               &span sp, &fn_ident i, node_id id) {
    assert (ccx.fm.contains_key(id));
    auto fcx = rec(enclosing=ccx.fm.get(id), id=id,
                   name=fn_ident_to_string(id, i), ccx=ccx);
    find_pre_post_fn(fcx, f);
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
